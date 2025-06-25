use std::fs;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::extract::Multipart;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use tempfile::tempfile;

// Max 20 MB upload size
const MAX_UPLOAD_SIZE: usize = 20 * 1024 * 1024;
const STORAGE_PATH: &'static str = "./package_data";

#[tokio::main]
async fn main() -> Result<()> {
    let app = Router::new().route("/", get(root)).route(
        "/publish",
        post(upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
    );
    let port = std::env::var("PORT").unwrap_or("3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("Listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> String {
    format!("hello world")
}

async fn upload(mut multipart: Multipart) -> Result<(), StatusCode> {
    let mut expected_hash = None;
    let mut tarball_data = None;
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().ok_or(StatusCode::BAD_REQUEST)?;
        match name {
            "tarball" => {
                let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                tarball_data = Some(data);
            }
            "hash" => {
                let hash_text = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                expected_hash =
                    Some(blake3::Hash::from_hex(&hash_text).map_err(|_| StatusCode::BAD_REQUEST)?);
            }
            _ => {}
        }
    }
    // Verify we got all required fields
    let (expected_hash, tarball_data) = match (expected_hash, tarball_data) {
        (Some(e), Some(t)) => (e, t),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let mut tarball = tempfile().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tarball
        .write_all(&tarball_data)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tarball
        .seek(SeekFrom::Start(0))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let actual_hash =
        common::hash_tarball(&tarball).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if expected_hash != actual_hash {
        println!("WARNING: hash mismatch for uploaded package");
        return Err(StatusCode::BAD_REQUEST);
    }
    // otherwise write our tarball to file
    let storage_path = std::env::current_dir()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .join(STORAGE_PATH);
    let target_path = storage_path.join(format!("{}.tar", expected_hash.to_string()));
    if target_path.exists() {
        println!(
            "WARNING: package already exists with hash: {}",
            actual_hash.to_string()
        );
        return Err(StatusCode::BAD_REQUEST);
    }
    fs::write(target_path, tarball_data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(())
}

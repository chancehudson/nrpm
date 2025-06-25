use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::routing::post;

mod error;
mod publish;

pub use error::OnyxError;

// Max 20 MB upload size
const MAX_UPLOAD_SIZE: usize = 20 * 1024 * 1024;
const STORAGE_PATH: &'static str = "./package_data";

#[tokio::main]
async fn main() -> Result<()> {
    let app = Router::new().route("/", get(root)).route(
        "/publish",
        post(publish::publish).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
    );
    let port = std::env::var("PORT").unwrap_or("3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("Listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> String {
    format!("Hello world!")
}

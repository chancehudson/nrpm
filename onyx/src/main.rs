use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::routing::post;
use redb::Database;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

use onyx_api::prelude::*;

mod auth;
mod download;
mod error;
mod list_packages;
mod publish;
#[cfg(test)]
mod tests;
mod user;

pub use error::OnyxError;

// Max 20 MB upload size
const MAX_UPLOAD_SIZE: usize = 20 * 1024 * 1024;
const STORAGE_PATH: &'static str = "./package_data";

#[derive(Clone)]
struct OnyxState {
    pub db: Arc<Database>,
    pub storage: OnyxStorage,
}

#[tokio::main]
async fn main() -> Result<()> {
    let db = Arc::new(Database::create("./db.redb")?);
    create_tables(db.clone())?;

    let app = build_server(OnyxState {
        db,
        storage: OnyxStorage::new(PathBuf::from(STORAGE_PATH))?,
    });
    let port = std::env::var("PORT").unwrap_or("3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("Listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn create_tables(db: Arc<redb::Database>) -> Result<()> {
    let write = db.begin_write()?;

    write.open_table(AUTH_TOKEN_TABLE)?;
    write.open_table(USER_TABLE)?;
    write.open_table(USERNAME_USER_ID_TABLE)?;
    write.open_table(PACKAGE_TABLE)?;
    write.open_table(PACKAGE_NAME_TABLE)?;
    write.open_table(PACKAGE_VERSION_NAME_TABLE)?;
    write.open_multimap_table(PACKAGE_VERSION_TABLE)?;
    write.open_table(VERSION_TABLE)?;

    write.commit()?;
    Ok(())
}

fn build_server(state: OnyxState) -> axum::Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    Router::new()
        .route("/", get(root))
        .route("/packages", get(list_packages::list_packages))
        .route(
            "/publish",
            post(publish::publish).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route("/signup", post(auth::signup))
        .route("/login", post(auth::login))
        .route("/auth", post(user::current_auth))
        .route("/propose_token", post(user::propose_token))
        .route("/version/{id}", get(download::download_package))
        .with_state(state)
        .layer(cors)
}

async fn root() -> String {
    format!("Hello world!")
}

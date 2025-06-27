use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::routing::post;
use common::timestamp;
use db::PackageModel;
use db::PackageVersionModel;
use db::UserModel;
use redb::Database;
use redb::MultimapTableDefinition;
use redb::TableDefinition;
use tempfile::TempDir;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

mod auth;
mod error;
mod list_packages;
mod publish;
#[cfg(test)]
mod tests;

pub use error::OnyxError;

// Max 20 MB upload size
const MAX_UPLOAD_SIZE: usize = 20 * 1024 * 1024;
const STORAGE_PATH: &'static str = "./package_data";

type NanoId<'a> = &'a str;
// auth token keyed to expiration timestamp
const AUTH_TOKEN_TABLE: TableDefinition<NanoId, (NanoId, u64)> =
    TableDefinition::new("auth_tokens");
// user_id keyed to user document
const USER_TABLE: TableDefinition<NanoId, UserModel> = TableDefinition::new("users");
// username keyed to user_id
const USERNAME_USER_ID_TABLE: TableDefinition<&str, NanoId> =
    TableDefinition::new("username_user_id");

const PACKAGE_TABLE: TableDefinition<NanoId, PackageModel> = TableDefinition::new("packages");
// used to ensure package names are unique
// TODO: sort by semver ordering for efficient latest version lookups
const PACKAGE_NAME_TABLE: TableDefinition<&str, ()> = TableDefinition::new("package_names");
// used to prevent multiple versions with the same name for a single package
// (package_id, version_name) keyed to ()
const PACKAGE_VERSION_NAME_TABLE: TableDefinition<(NanoId, &str), ()> =
    TableDefinition::new("package_version_name");
// package_id keyed to many versions
const PACKAGE_VERSION_TABLE: MultimapTableDefinition<NanoId, NanoId> =
    MultimapTableDefinition::new("package_versions");
const VERSION_TABLE: TableDefinition<NanoId, PackageVersionModel> =
    TableDefinition::new("versions");

#[derive(Clone)]
struct OnyxState {
    pub db: Arc<Database>,
    pub storage_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let db = Arc::new(Database::create("./db.redb")?);
    create_tables(db.clone())?;

    let app = build_server(OnyxState {
        db,
        storage_path: PathBuf::from(STORAGE_PATH),
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
        .with_state(state)
        .layer(cors)
}

async fn root() -> String {
    format!("Hello world!")
}

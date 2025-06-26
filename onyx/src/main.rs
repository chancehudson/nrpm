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
use redb::Table;
use redb::TableDefinition;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

mod auth;
mod error;
mod list_packages;
mod publish;

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
const PACKAGE_NAME_TABLE: TableDefinition<&str, ()> = TableDefinition::new("package_names");
const PACKAGE_TABLE: TableDefinition<NanoId, PackageModel> = TableDefinition::new("packages");
const PACKAGE_VERSION_TABLE: TableDefinition<NanoId, PackageVersionModel> =
    TableDefinition::new("package_versions");

#[derive(Clone)]
struct OnyxState {
    pub db: Arc<Database>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let db = Arc::new(Database::create("./db.redb")?);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let app = Router::new()
        .route("/", get(root))
        .route("/packages", get(list_packages::list_packages))
        .route(
            "/publish",
            post(publish::publish).layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE)),
        )
        .route("/signup", post(auth::signup))
        .route("/login", post(auth::login))
        .with_state(OnyxState { db })
        .layer(cors);
    let port = std::env::var("PORT").unwrap_or("3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("Listening on port {port}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> String {
    format!("Hello world!")
}

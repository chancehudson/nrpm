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
use redb::ReadableTable;
use redb::TableDefinition;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

mod auth;
mod error;
mod publish;

pub use error::OnyxError;

// Max 20 MB upload size
const MAX_UPLOAD_SIZE: usize = 20 * 1024 * 1024;
const STORAGE_PATH: &'static str = "./package_data";

// auth token keyed to expiration timestamp
const AUTH_TOKEN_TABLE: TableDefinition<&str, (u128, u64)> = TableDefinition::new("auth_tokens");
// user_id keyed to user document
const USER_TABLE: TableDefinition<u128, UserModel> = TableDefinition::new("users");
// username keyed to user_id
const USERNAME_USER_ID_TABLE: TableDefinition<&str, u128> =
    TableDefinition::new("username_user_id");
const PACKAGE_TABLE: TableDefinition<u128, PackageModel> = TableDefinition::new("packages");
const PACKAGE_VERSION_TABLE: TableDefinition<u128, PackageVersionModel> =
    TableDefinition::new("package_versions");

pub fn rand_key<V>(table: &redb::Table<u128, V>) -> Result<u128>
where
    V: redb::Value,
{
    let mut id: u128;
    loop {
        id = rand::random();
        if table.get(id)?.is_none() {
            break;
        }
    }
    Ok(id)
}

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

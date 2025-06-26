use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::routing::post;
use redb::Database;
use redb::TableDefinition;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

mod auth;
mod error;
mod publish;

pub use error::OnyxError;
use redb::Value;

// Max 20 MB upload size
const MAX_UPLOAD_SIZE: usize = 20 * 1024 * 1024;
const STORAGE_PATH: &'static str = "./package_data";

// auth token keyed to expiration timestamp
const AUTH_TOKEN_TABLE: TableDefinition<&str, (u128, u64)> = TableDefinition::new("auth_tokens");
// username id keyed to user document
const USER_TABLE: TableDefinition<u128, User> = TableDefinition::new("users");
const USERNAME_USER_ID_TABLE: TableDefinition<&str, u128> =
    TableDefinition::new("username_user_id");

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
struct User {
    pub id: u128,
    pub username: String,
    pub created_at: u64,
    pub password_hash: String,
}

impl Value for User {
    type SelfType<'a> = User;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None // Variable width due to strings
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        bincode::deserialize(data).expect("Failed to deserialize User")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a> {
        bincode::serialize(value).expect("Failed to serialize User")
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("User")
    }
}

pub fn timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
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

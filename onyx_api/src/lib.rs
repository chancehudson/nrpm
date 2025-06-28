pub mod db;
pub mod http;
pub mod prelude;
#[cfg(feature = "server")]
mod storage;

#[cfg(feature = "server")]
use storage::*;

pub use http::OnyxApi;

#[cfg(debug_assertions)]
pub const REGISTRY_URL: &'static str = "http://127.0.0.1:3000";
#[cfg(not(debug_assertions))]
pub const REGISTRY_URL: &'static str = "https://api.nrpm.io";

pub fn timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

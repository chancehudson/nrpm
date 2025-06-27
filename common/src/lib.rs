mod api;
pub mod api_types;
mod tarball;

pub use api::OnyxApi;
pub use tarball::create_tarball;
pub use tarball::hash_tarball;

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

use serde::Deserialize;
use serde::Serialize;

use crate::user::UserModelSafe;

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct LoginResponse {
    pub user: UserModelSafe,
    pub token: String,
    pub expires_at: u64,
}

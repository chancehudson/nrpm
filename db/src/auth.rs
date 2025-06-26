use serde::Deserialize;
use serde::Serialize;

use super::UserModel;

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    pub user: UserModel,
    pub token: String,
    pub expires_at: u64,
}

use serde::Deserialize;
use serde::Serialize;

use crate::db::UserModelSafe;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct TokenOnly {
    pub token: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct ProposeToken {
    pub token: String,
    pub proposed_token: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct PublishData {
    pub hash: String,
    pub token: String,
    pub package_id: Option<String>, // None if creating a new package
    pub package_name: String,
    pub version_name: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct PublishResponse {
    pub package_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct LoginResponse {
    pub user: UserModelSafe,
    pub token: String,
    pub expires_at: u64,
}

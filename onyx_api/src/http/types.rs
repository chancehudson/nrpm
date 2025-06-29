use nanoid::nanoid;
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
    pub package_name: String,
    pub version_name: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct PublishResponse {
    pub package_id: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

impl Default for LoginRequest {
    fn default() -> Self {
        Self {
            username: nanoid!(),
            password: nanoid!(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct LoginResponse {
    pub user: UserModelSafe,
    pub token: String,
    pub expires_at: u64,
}

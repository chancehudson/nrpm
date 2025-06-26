use anyhow::Result;
use axum::extract::Json;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use bcrypt::DEFAULT_COST;
use bcrypt::hash;
use nanoid::nanoid;
use redb::ReadableTable;
use serde::Deserialize;
use serde::Serialize;

use crate::USER_TABLE;
use crate::USERNAME_USER_ID_TABLE;
use crate::User;
use crate::timestamp;

use super::AUTH_TOKEN_TABLE;
use super::OnyxError;
use super::OnyxState;

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    pub success: bool,
    pub message: String,
    pub token: String,
    pub expires_at: u64,
}
pub async fn login(
    State(state): State<OnyxState>,
    Json(payload): Json<LoginRequest>,
) -> Result<ResponseJson<LoginResponse>, OnyxError> {
    let read = state.db.begin_write()?;
    let username_table = read.open_table(USERNAME_USER_ID_TABLE)?;
    let user_id = match username_table.get(payload.username.as_str())? {
        Some(id) => id.value(),
        None => return Err(OnyxError::bad_request("username not registered")),
    };
    drop(username_table);
    let user_table = read.open_table(USER_TABLE)?;
    let user = match user_table.get(user_id)? {
        Some(user) => user.value(),
        None => {
            return Err(OnyxError::bad_request(
                "username registered without user document. This is an internal error",
            ));
        }
    };
    drop(user_table);
    drop(read);
    if !bcrypt::verify(payload.password, &user.password_hash)? {
        return Err(OnyxError::bad_request("bad password"));
    }

    let token = nanoid!();
    let expires_at = timestamp() + 3600;
    let write = state.db.begin_write()?;
    let mut auth_token_table = write.open_table(AUTH_TOKEN_TABLE)?;
    auth_token_table.insert(token.as_str(), (user.id, expires_at))?;
    drop(auth_token_table);
    write.commit()?;

    Ok(ResponseJson(LoginResponse {
        success: true,
        message: "Login successful".to_string(),
        token,
        expires_at,
    }))
}

pub async fn signup(
    State(state): State<OnyxState>,
    Json(payload): Json<LoginRequest>,
) -> Result<ResponseJson<LoginResponse>, OnyxError> {
    let password_hash = hash(payload.password, DEFAULT_COST)?;
    let write = state.db.begin_write()?;
    let mut username_table = write.open_table(USERNAME_USER_ID_TABLE)?;
    if let Some(_) = username_table.get(payload.username.as_str())? {
        return Err(OnyxError::bad_request("username is already in use"));
    }
    let mut user_table = write.open_table(USER_TABLE)?;
    let user_id: u128 = rand::random();
    if user_table.get(user_id)?.is_some() {
        panic!("duplicate user id generated lol");
    }
    let user = User {
        username: payload.username,
        id: user_id,
        created_at: timestamp(),
        password_hash,
    };
    username_table.insert(user.username.as_str(), user.id)?;
    user_table.insert(user_id, user)?;

    let mut auth_token_table = write.open_table(AUTH_TOKEN_TABLE)?;
    let token = nanoid!();
    let expires_at = timestamp() + 3600;
    auth_token_table.insert(token.as_str(), (user_id, expires_at))?;
    drop(username_table);
    drop(user_table);
    drop(auth_token_table);
    write.commit()?;

    Ok(ResponseJson(LoginResponse {
        success: true,
        message: "Signup successful".to_string(),
        token,
        expires_at,
    }))
}

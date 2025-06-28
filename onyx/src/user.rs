use anyhow::Result;
use axum::extract::Json;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use nanoid::nanoid;
use reqwest::StatusCode;

use onyx_api::prelude::*;

use super::AUTH_TOKEN_TABLE;
use super::OnyxError;
use super::OnyxState;
use super::USER_TABLE;

fn is_safe_nanoid(input: &str) -> bool {
    input.chars().all(|c| nanoid::alphabet::SAFE.contains(&c))
}

// TODO: make this a constant or a lazy cell
fn default_nanoid_len() -> usize {
    nanoid!().len()
}

pub async fn current_auth(
    State(state): State<OnyxState>,
    Json(payload): Json<TokenOnly>,
) -> Result<ResponseJson<LoginResponse>, OnyxError> {
    let read = state.db.begin_read()?;
    let auth_table = read.open_table(AUTH_TOKEN_TABLE)?;
    let user_table = read.open_table(USER_TABLE)?;
    let (user_id, expires_at) = if let Some(entry) = auth_table.get(payload.token.as_str())? {
        let (user_id, expires_at) = entry.value();
        if timestamp() > expires_at {
            return Err(OnyxError::bad_request("Expired token!"));
        }
        (user_id.to_string(), expires_at)
    } else {
        return Err(OnyxError::bad_request("Invalid token!"));
    };
    let user = user_table.get(user_id.as_str())?.unwrap().value();
    Ok(ResponseJson(LoginResponse {
        user: UserModelSafe::from(user),
        token: payload.token,
        expires_at,
    }))
}

pub async fn propose_token(
    State(state): State<OnyxState>,
    Json(payload): Json<ProposeToken>,
) -> Result<StatusCode, OnyxError> {
    if !is_safe_nanoid(&payload.proposed_token) {
        return Err(OnyxError::bad_request("Token contains invalid characters"));
    }
    if payload.proposed_token.len() != default_nanoid_len() {
        return Err(OnyxError::bad_request(&format!(
            "Token must be {} characters",
            default_nanoid_len(),
        )));
    }
    let read = state.db.begin_read()?;
    let auth_table = read.open_table(AUTH_TOKEN_TABLE)?;
    let user_id = if let Some(entry) = auth_table.get(payload.token.as_str())? {
        let (user_id, expires_at) = entry.value();
        if timestamp() > expires_at {
            return Err(OnyxError::bad_request("Expired token!"));
        }
        user_id.to_string()
    } else {
        return Err(OnyxError::bad_request("Invalid token!"));
    };

    let expires_at = timestamp() + 3600;
    let write = state.db.begin_write()?;
    {
        let mut auth_token_table = write.open_table(AUTH_TOKEN_TABLE)?;
        auth_token_table.insert(
            payload.proposed_token.as_str(),
            (user_id.as_str(), expires_at),
        )?;
    }
    write.commit()?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::AUTH_TOKEN_TABLE;
    use crate::tests::OnyxTestState;
    use anyhow::Result;
    use nanoid::nanoid;
    use onyx_api::timestamp;

    #[tokio::test]
    async fn fail_auth_bad_token() -> Result<()> {
        let test = OnyxTestState::new().await?;

        let e = test
            .api
            .auth("nonsense token of course".to_string())
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "Invalid token!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_auth_expired_token() -> Result<()> {
        let test = OnyxTestState::new().await?;

        let (login, _password) = test.signup(None).await?;

        // write an expired token to the db
        let expired_token = {
            let token = nanoid!();
            let expires_at = timestamp() - 1;

            let write = test.state.db.begin_write().unwrap();
            let mut auth_table = write.open_table(AUTH_TOKEN_TABLE).unwrap();
            auth_table
                .insert(token.as_str(), (login.user.id.as_str(), expires_at))
                .unwrap();
            drop(auth_table);
            write.commit()?;

            token
        };

        let e = test.api.auth(expired_token).await.unwrap_err();
        assert_eq!(e.to_string(), "Expired token!");
        Ok(())
    }
}

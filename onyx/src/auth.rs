use anyhow::Result;
use axum::extract::Json;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use bcrypt::DEFAULT_COST;
use bcrypt::hash;
use nanoid::nanoid;
use redb::ReadableTable;

use onyx_api::prelude::*;

use super::AUTH_TOKEN_TABLE;
use super::OnyxError;
use super::OnyxState;
use super::USER_TABLE;
use super::USERNAME_USER_ID_TABLE;

const MIN_PASSWORD_LEN: usize = 10;

pub async fn login(
    State(state): State<OnyxState>,
    Json(payload): Json<LoginRequest>,
) -> Result<ResponseJson<LoginResponse>, OnyxError> {
    let user = {
        let read = state.db.begin_read()?;
        let username_table = read.open_table(USERNAME_USER_ID_TABLE)?;
        let user_table = read.open_table(USER_TABLE)?;

        let user_id = match username_table.get(payload.username.as_str())? {
            Some(id) => id.value().to_string(),
            None => return Err(OnyxError::bad_request("username not registered")),
        };

        match user_table.get(user_id.as_str())? {
            Some(user) => user.value(),
            None => {
                return Err(OnyxError::bad_request(
                    "username registered without user document. This is an internal error",
                ));
            }
        }
    };

    match bcrypt::verify(payload.password, &user.password_hash) {
        Ok(success) => {
            if !success {
                return Err(OnyxError::bad_request("bad password"));
            }
        }
        Err(e) => {
            println!("bcrypt error: {}", e);
            return Err(OnyxError::bad_request("bad password"));
        }
    }

    let token = nanoid!();
    let expires_at = timestamp() + 3600;

    let write = state.db.begin_write()?;
    {
        let mut auth_token_table = write.open_table(AUTH_TOKEN_TABLE)?;
        auth_token_table.insert(token.as_str(), (user.id.as_str(), expires_at))?;
    }
    write.commit()?;

    Ok(ResponseJson(LoginResponse {
        user: UserModelSafe::from(user),
        token,
        expires_at,
    }))
}

pub async fn signup(
    State(state): State<OnyxState>,
    Json(payload): Json<LoginRequest>,
) -> Result<ResponseJson<LoginResponse>, OnyxError> {
    if payload.password.len() < MIN_PASSWORD_LEN {
        return Err(OnyxError::bad_request(&format!(
            "password must be more than {MIN_PASSWORD_LEN} characters"
        )));
    }
    let password_hash = hash(payload.password, DEFAULT_COST)?;
    let write = state.db.begin_write()?;
    let mut username_table = write.open_table(USERNAME_USER_ID_TABLE)?;

    if let Some(_) = username_table.get(payload.username.as_str())? {
        return Err(OnyxError::bad_request("username is already in use"));
    }

    let user = UserModel {
        username: payload.username,
        id: nanoid!(),
        created_at: timestamp(),
        password_hash,
    };
    let token = nanoid!();
    let expires_at = timestamp() + 3600;

    {
        let mut user_table = write.open_table(USER_TABLE)?;
        let mut auth_token_table = write.open_table(AUTH_TOKEN_TABLE)?;
        username_table.insert(user.username.as_str(), user.id.as_str())?;
        user_table.insert(user.id.as_str(), user.clone())?;
        auth_token_table.insert(token.as_str(), (user.id.as_str(), expires_at))?;
        drop(username_table);
    }
    write.commit()?;

    Ok(ResponseJson(LoginResponse {
        user: UserModelSafe::from(user),
        token,
        expires_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tests::OnyxTest;
    use anyhow::Result;

    #[tokio::test]
    async fn should_signup_login() -> Result<()> {
        let test = OnyxTest::new().await?;

        let (login, password) = test.signup(None).await?;

        println!(
            "Created user \"{}\" with password: \"{}\"",
            login.user.username, password
        );

        let login2 = test
            .login(Some(LoginRequest {
                username: login.user.username.clone(),
                password,
            }))
            .await?;

        // user should match
        assert!(login2.user == login.user);
        // tokens should mismatch
        assert!(login != login2);

        Ok(())
    }

    #[tokio::test]
    async fn fail_signup_short_password() -> Result<()> {
        let test = OnyxTest::new().await?;
        const TEST_PASSWORD_LEN: usize = MIN_PASSWORD_LEN - 1;
        let e = test
            .signup(Some(LoginRequest {
                username: nanoid!(),
                password: nanoid!(TEST_PASSWORD_LEN),
            }))
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "password must be more than 10 characters");
        Ok(())
    }

    #[tokio::test]
    async fn fail_login_bad_username() -> Result<()> {
        let test = OnyxTest::new().await?;

        // test.login(Some(LoginRequest { username: "not_a_user", password: "not_a_password" }))
        let e = test.login(None).await.unwrap_err();
        assert_eq!(e.to_string(), "username not registered");
        Ok(())
    }

    #[tokio::test]
    async fn fail_login_bad_password() -> Result<()> {
        let test = OnyxTest::new().await?;
        let (login, _password) = test.signup(None).await?;

        let e = test
            .login(Some(LoginRequest {
                username: login.user.username,
                password: nanoid!(),
            }))
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "bad password");
        Ok(())
    }

    #[tokio::test]
    async fn should_double_register_username() -> Result<()> {
        let test = OnyxTest::new().await?;

        let username = nanoid!();
        let (login, password) = test
            .signup(Some(LoginRequest {
                username: username.clone(),
                password: nanoid!(),
            }))
            .await?;

        assert_eq!(login.user.username, username);

        let e = test
            .signup(Some(LoginRequest {
                username: login.user.username,
                password,
            }))
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "username is already in use");
        Ok(())
    }
}

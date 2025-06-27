use anyhow::Result;
use reqwest::multipart;
use serde_json::json;

use crate::REGISTRY_URL;

use super::api_types::LoginRequest;
use super::api_types::LoginResponse;
use super::api_types::PublishData;
use super::api_types::PublishResponse;

pub struct OnyxApi {
    pub url: String,
}

impl Default for OnyxApi {
    fn default() -> Self {
        Self {
            url: REGISTRY_URL.to_string(),
        }
    }
}

impl OnyxApi {
    pub fn new(url: String) -> Result<Self> {
        Ok(Self { url })
    }

    /// Generate a user with random username and password. Returns
    /// the `UserModel` and the password.
    pub async fn signup(&self, request: LoginRequest) -> Result<LoginResponse> {
        let response = reqwest::Client::new()
            .post(format!("{}/signup", self.url))
            .json(&request)
            .send()
            .await?;
        if response.status().is_success() {
            let data: LoginResponse = response.json().await?;

            #[cfg(test)]
            assert!(data.user.created_at.abs_diff(super::timestamp()) < 10); // timestamp should be sane

            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }

    pub async fn login(&self, request: LoginRequest) -> Result<LoginResponse> {
        let response = reqwest::Client::new()
            .post(format!("{}/login", self.url))
            .json(&json!(request))
            .send()
            .await?;
        if response.status().is_success() {
            let data: LoginResponse = response.json().await?;
            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }

    pub async fn publish(&self, request: PublishData, tarball: Vec<u8>) -> Result<PublishResponse> {
        let form = multipart::Form::new()
            .part(
                "tarball",
                multipart::Part::bytes(tarball)
                    .file_name("package.tar")
                    .mime_str("application/tar")?,
            )
            .part(
                "publish_data",
                multipart::Part::bytes(bincode::serialize(&request)?),
            );
        let response = reqwest::Client::new()
            .post(format!("{}/publish", self.url))
            .multipart(form)
            .send()
            .await?;
        if response.status().is_success() {
            let data: PublishResponse = response.json().await?;
            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }
}

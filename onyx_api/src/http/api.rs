use anyhow::Result;
use reqwest::multipart;
use serde_json::json;

use super::types::*;
use crate::REGISTRY_URL;
use crate::db::*;

#[derive(Clone, Debug)]
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

    pub fn version_download_url(&self, id: HashId) -> String {
        format!("{}/v0/version/{}", self.url, id.to_string())
    }

    pub async fn load_packages(&self) -> Result<Vec<(PackageModel, PackageVersionModel)>> {
        let response = reqwest::Client::new()
            .get(format!("{}/v0/packages", self.url))
            .send()
            .await?;
        if response.status().is_success() {
            let data = response.json().await?;
            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }

    pub async fn auth(&self, token: String) -> Result<LoginResponse> {
        let response = reqwest::Client::new()
            .post(format!("{}/v0/auth", self.url))
            .json(&TokenOnly { token })
            .send()
            .await?;
        if response.status().is_success() {
            let data: LoginResponse = response.json().await?;
            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }

    pub async fn propose_token(&self, proposed_token: String, token: String) -> Result<()> {
        let response = reqwest::Client::new()
            .post(format!("{}/v0/propose_token", self.url))
            .json(&ProposeToken {
                token,
                proposed_token,
            })
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }

    /// Generate a user with random username and password. Returns
    /// the `UserModel` and the password.
    pub async fn signup(&self, request: LoginRequest) -> Result<LoginResponse> {
        let response = reqwest::Client::new()
            .post(format!("{}/v0/signup", self.url))
            .json(&request)
            .send()
            .await?;
        if response.status().is_success() {
            let data: LoginResponse = response.json().await?;

            #[cfg(test)]
            assert!(data.user.created_at.abs_diff(crate::timestamp()) < 10); // timestamp should be sane

            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }

    pub async fn login(&self, request: LoginRequest) -> Result<LoginResponse> {
        let response = reqwest::Client::new()
            .post(format!("{}/v0/login", self.url))
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

    #[cfg(feature = "publish")]
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
                // idk about this use of bincode
                // feels like a serialize to json
                //
                // ehhh no publish from web
                multipart::Part::bytes(bincode::serialize(&request)?),
            );
        let response = reqwest::Client::new()
            .post(format!("{}/v0/publish", self.url))
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

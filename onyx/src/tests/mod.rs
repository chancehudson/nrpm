use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use common::create_tarball;
use common::hash_tarball;
use common::timestamp;
use db::LoginRequest;
use db::LoginResponse;
use nanoid::nanoid;
use reqwest::multipart;
use serde_json::json;
use tempfile::TempDir;

use crate::publish::PublishData;

use super::OnyxState;
use super::build_server;
use super::create_tables;

pub struct OnyxTestState {
    pub url: String,
    pub tmpdir: PathBuf,
    pub state: OnyxState,
}

impl OnyxTestState {
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new().unwrap();

        let db_path = temp_dir.path().join(format!("{}.db", nanoid!()));
        let db = Arc::new(redb::Database::create(&db_path).unwrap());

        println!("creating tables");
        create_tables(db.clone())?;

        println!("building server");
        let app = build_server(db.clone());

        println!("starting TcpListener");
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:0")).await?;
        let addr = listener.local_addr()?.to_string();
        println!("spawning server thread");
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        println!("waiting");
        tokio::time::sleep(Duration::from_millis(500)).await;

        let state = OnyxState { db };

        Ok(Self {
            url: format!("http://{}", addr),
            state,
            tmpdir: temp_dir.path().to_path_buf(),
        })
    }

    /// Generate a user with random username and password. Returns
    /// the `UserModel` and the password.
    pub async fn signup(&self, request: Option<LoginRequest>) -> Result<(LoginResponse, String)> {
        let request = request.unwrap_or(LoginRequest {
            username: nanoid!(),
            password: nanoid!(),
        });
        let password = request.password.clone();
        let response = reqwest::Client::new()
            .post(format!("{}/signup", self.url))
            .json(&json!(request))
            .send()
            .await?;
        if response.status().is_success() {
            let data: LoginResponse = response.json().await?;

            assert!(data.user.created_at.abs_diff(timestamp()) < 10); // timestamp should be sane

            Ok((data, password))
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }

    pub async fn login(&self, request: Option<LoginRequest>) -> Result<LoginResponse> {
        let request = request.unwrap_or(LoginRequest {
            username: nanoid!(),
            password: nanoid!(),
        });
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

    // Test helper to create a test tarball
    pub fn create_test_tarball() -> Result<(Vec<u8>, blake3::Hash)> {
        let workdir = tempfile::TempDir::new()?;
        std::fs::write(workdir.path().join("aaaaa"), "testcontents\n")?;
        let tarball = create_tarball(workdir.path().to_path_buf())?;
        let mut tarball_clone = tarball.try_clone()?;
        let hash = hash_tarball(&tarball)?;
        let mut tarball_bytes = vec![];
        tarball_clone.read_to_end(&mut tarball_bytes)?;
        println!("tarball len {}", tarball_bytes.len());
        Ok((tarball_bytes, hash))
    }

    pub async fn publish(
        &self,
        request: Option<PublishData>,
        tarball: (Vec<u8>, blake3::Hash),
    ) -> Result<PublishData> {
        let data = request.unwrap_or(PublishData {
            hash: tarball.1.to_string(),
            token: nanoid!(),
            package_id: None,
            package_name: nanoid!(),
            version_name: nanoid!(),
        });
        let form = multipart::Form::new()
            .part(
                "tarball",
                multipart::Part::bytes(tarball.0.clone())
                    .file_name("package.tar")
                    .mime_str("application/tar")?,
            )
            .part(
                "publish_data",
                multipart::Part::bytes(bincode::serialize(&data)?),
            );
        let response = reqwest::Client::new()
            .post(format!("{}/publish", self.url))
            .multipart(form)
            .send()
            .await?;
        if response.status().is_success() {
            assert_eq!(response.status(), 204);
            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }
}

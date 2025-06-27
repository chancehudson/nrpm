use std::io::Read;
use std::io::Seek;
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
use tempfile::tempfile;

use crate::publish::PublishData;
use crate::publish::PublishResponse;

use super::OnyxState;
use super::build_server;
use super::create_tables;

pub struct OnyxTestState {
    pub url: String,
    pub state: OnyxState,

    #[allow(dead_code)]
    tmp_handles: Vec<TempDir>,
}

impl OnyxTestState {
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;

        let db_path = temp_dir.path().join(format!("{}.db", nanoid!()));
        let db = Arc::new(redb::Database::create(&db_path).unwrap());

        create_tables(db.clone())?;

        let storage_dir = TempDir::new()?;
        let storage_path = storage_dir.path().to_path_buf();
        let state = OnyxState { db, storage_path };
        let app = build_server(state.clone());

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:0")).await?;
        let addr = listener.local_addr()?.to_string();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(Self {
            url: format!("http://{}", addr),
            state,

            // used to keep handles in memory to prevent directory removal until end of program
            tmp_handles: vec![temp_dir, storage_dir],
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
    pub fn create_test_tarball(content: Option<&str>) -> Result<(Vec<u8>, blake3::Hash)> {
        let content = content.unwrap_or("testcontents\n");
        let workdir = tempfile::TempDir::new()?;
        std::fs::write(workdir.path().join("aaaaa"), content)?;
        let tar_file = tempfile()?;
        let tarball = create_tarball(workdir.path().to_path_buf(), tar_file)?;
        let mut tarball_clone = tarball.try_clone()?;
        let hash = hash_tarball(&tarball)?;
        // Explicitly seek the clone to the beginning
        tarball_clone.seek(std::io::SeekFrom::Start(0))?;
        let mut tarball_bytes = vec![];
        tarball_clone.read_to_end(&mut tarball_bytes)?;
        Ok((tarball_bytes, hash))
    }

    pub async fn publish(
        &self,
        request: Option<PublishData>,
        tarball: (Vec<u8>, blake3::Hash),
    ) -> Result<PublishResponse> {
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
            let data: PublishResponse = response.json().await?;
            Ok(data)
        } else {
            anyhow::bail!("{}", response.text().await?);
        }
    }
}

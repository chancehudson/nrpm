use std::fs;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

use anyhow::Result;
use axum::extract::Multipart;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use db::PackageModel;
use db::PackageVersionModel;
use nanoid::nanoid;
use redb::ReadableTable;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use tempfile::tempfile;

use crate::PACKAGE_NAME_TABLE;
use crate::PACKAGE_VERSION_NAME_TABLE;
use crate::VERSION_TABLE;

use super::AUTH_TOKEN_TABLE;
use super::OnyxError;
use super::OnyxState;
use super::PACKAGE_TABLE;
use super::PACKAGE_VERSION_TABLE;
use super::timestamp;

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

pub async fn publish(
    State(state): State<OnyxState>,
    mut multipart: Multipart,
) -> Result<ResponseJson<PublishResponse>, OnyxError> {
    let mut tarball_data = None;
    let mut publish_data: Option<PublishData> = None;
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().ok_or(OnyxError::bad_request(
            "All fields in multipart upload must have names",
        ))?;
        match name {
            "tarball" => {
                let data = field.bytes().await?;
                tarball_data = Some(data);
            }
            "publish_data" => {
                let bytes = field.bytes().await?;
                publish_data = Some(
                    bincode::deserialize(&bytes)
                        .map_err(|_| OnyxError::bad_request("Failed to decode publish data!"))?,
                );
            }
            _ => {}
        }
    }
    // Verify we got all required fields
    let (tarball_data, publish_data) = match (tarball_data, publish_data) {
        (Some(e), Some(p)) => (e, p),
        _ => {
            return Err(OnyxError::bad_request(
                "Publish request missing field, expected: \"tarball\", \"publish_data\"",
            ));
        }
    };
    let read = state.db.begin_read()?;
    let auth_table = read.open_table(AUTH_TOKEN_TABLE)?;
    let package_table = read.open_table(PACKAGE_TABLE)?;
    let user_id = if let Some(entry) = auth_table.get(publish_data.token.as_str())? {
        let (user_id, expires_at) = entry.value();
        if timestamp() > expires_at {
            return Err(OnyxError::bad_request(
                "Publish request contains invalid token!",
            ));
        }
        user_id.to_string()
    } else {
        return Err(OnyxError::bad_request(
            "Publish request contains invalid token!",
        ));
    };
    if let Some(package_id) = &publish_data.package_id {
        if let Some(package) = package_table.get(package_id.as_str())? {
            let package = package.value();
            if package.author_id != user_id {
                return Err(OnyxError::bad_request("Not the package author!"));
            }
            if package.name != publish_data.package_name {
                return Err(OnyxError::bad_request(
                    "Package name mismatch in publish request!",
                ));
            }
        } else {
            return Err(OnyxError::bad_request("Package does not exist for id!"));
        };
    }

    // now we're authed, and confirmed to be the author of the package
    // let's examine the provided tarball
    let mut tarball = tempfile()?;
    tarball.write_all(&tarball_data)?;
    tarball.flush()?;
    tarball.sync_all()?;
    tarball.seek(SeekFrom::Start(0))?;
    let actual_hash = common::hash_tarball(&tarball)?;

    if blake3::Hash::from_hex(publish_data.hash)? != actual_hash {
        println!("WARNING: hash mismatch for uploaded package, computed: {actual_hash}");
        return Err(OnyxError::bad_request(
            "Hash mismatch for uploaded tarball!",
        ));
    }

    let target_path = state
        .storage_path
        .join(format!("{}.tar", actual_hash.to_string()));
    if target_path.exists() {
        println!(
            "WARNING: package already exists with hash: {}",
            actual_hash.to_string()
        );
        return Err(OnyxError::bad_request(&format!(
            "Package with hash already exists: {}",
            actual_hash.to_string()
        )));
    }
    fs::write(target_path, tarball_data)?;

    // now write our package to the db
    let write = state.db.begin_write()?;
    let package = {
        let mut package_table = write.open_table(PACKAGE_TABLE)?;
        let mut package_version_table = write.open_multimap_table(PACKAGE_VERSION_TABLE)?;
        let mut version_table = write.open_table(VERSION_TABLE)?;
        let mut package_name_table = write.open_table(PACKAGE_NAME_TABLE)?;
        let mut package_version_name_table = write.open_table(PACKAGE_VERSION_NAME_TABLE)?;

        // do the name availability check here to avoid a race condition
        // e.g. check for name before starting write, another threads takes name, this thread overwrites name
        if publish_data.package_id.is_none() {
            // creating a new package, verify that name is available
            if let Some(_) = package_name_table.get(publish_data.package_name.as_str())? {
                return Err(OnyxError::bad_request("Package name is already in use!"));
            }
        }
        package_name_table.insert(publish_data.package_name.as_str(), ())?;

        // generate a new version id for what is being published
        let version_id = nanoid!();

        let package = if let Some(package_id) = publish_data.package_id {
            // we confimed the package exists above so unwrap is safe here
            let mut package = package_table.get(package_id.as_str())?.unwrap().value();
            package.latest_version_id = version_id.clone();
            package_table.insert(package_id.as_str(), package.clone())?;
            package
        } else {
            let package = PackageModel {
                id: nanoid!(),
                name: publish_data.package_name,
                author_id: user_id.clone(),
                latest_version_id: version_id.clone(),
            };
            package_table.insert(package.id.as_str(), package.clone())?;
            package
        };

        // make sure the version name is unique
        if let Some(_) = package_version_name_table
            .get((package.id.as_str(), publish_data.version_name.as_str()))?
        {
            return Err(OnyxError::bad_request(&format!(
                "Version already exists for package! version_name: {} package_name: {}",
                publish_data.version_name, package.name
            )));
        }

        package_version_name_table.insert(
            (package.id.as_str(), publish_data.version_name.as_str()),
            (),
        )?;
        package_version_table.insert(package.id.as_str(), version_id.as_str())?;
        version_table.insert(
            version_id.as_str(),
            PackageVersionModel {
                id: version_id.clone(),
                name: publish_data.version_name,
                author_id: user_id,
                package_id: package.id.clone(),
                hash: *actual_hash.as_bytes(),
                created_at: timestamp(),
            },
        )?;

        package
    };
    write.commit()?;

    Ok(ResponseJson(PublishResponse {
        package_id: package.id,
    }))
}

#[cfg(test)]
mod tests {
    use crate::tests::*;

    use super::*;
    use anyhow::Result;
    use reqwest::multipart;

    #[tokio::test]
    async fn test_connection() -> Result<()> {
        let test = OnyxTestState::new().await?;
        reqwest::Client::new().get(&test.url).send().await?;
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_without_token() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let tarball = OnyxTestState::create_test_tarball(None)?;

        let mut publish_data = PublishData::default();
        publish_data.hash = tarball.1.to_string();
        let e = test.publish(Some(publish_data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Publish request contains invalid token!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_expired_token() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login, _password) = test.signup(None).await?;
        let user_id = login.user.id;

        // write an expired token to the db
        let expired_token = {
            let token = nanoid!();
            let expires_at = timestamp() - 1;

            let write = test.state.db.begin_write().unwrap();
            let mut auth_table = write.open_table(AUTH_TOKEN_TABLE).unwrap();
            auth_table
                .insert(token.as_str(), (user_id.as_str(), expires_at))
                .unwrap();
            drop(auth_table);
            write.commit()?;

            token
        };

        let (tarball_bytes, hash) = OnyxTestState::create_test_tarball(None)?;
        let mut publish_data = PublishData::default();
        publish_data.hash = hash.to_string();
        publish_data.token = expired_token;
        let e = test
            .publish(Some(publish_data), (tarball_bytes, hash))
            .await
            .unwrap_err();
        assert_eq!(e.to_string(), "Publish request contains invalid token!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_deformed_data() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (tarball_bytes, _hash) = OnyxTestState::create_test_tarball(None)?;

        let form = multipart::Form::new()
            .part(
                "tarball",
                multipart::Part::bytes(tarball_bytes.clone())
                    .file_name("package.tar")
                    .mime_str("application/tar")?,
            )
            .part(
                "publish_data",
                multipart::Part::bytes("somebaddata".as_bytes()),
            );

        let response = reqwest::Client::new()
            .post(format!("{}/publish", test.url))
            .multipart(form)
            .send()
            .await?;

        assert_eq!(response.status().is_success(), false);
        let e = response.text().await?;
        assert_eq!(e.to_string(), "Failed to decode publish data!");

        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_without_fields() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (tarball_bytes, hash) = OnyxTestState::create_test_tarball(None)?;
        let client = reqwest::Client::new();

        let mut publish_data = PublishData::default();
        publish_data.hash = hash.to_string();
        let expected_error =
            "Publish request missing field, expected: \"tarball\", \"publish_data\"";
        {
            // without tarball
            let form = multipart::Form::new().part(
                "publish_data",
                multipart::Part::bytes(bincode::serialize(&publish_data)?),
            );
            let response = client
                .post(format!("{}/publish", test.url))
                .multipart(form)
                .send()
                .await?;
            if response.status().is_success() {
                assert!(false);
            }
            assert_eq!(response.text().await?, expected_error);
        }

        {
            // without publish data
            let form = multipart::Form::new().part(
                "tarball",
                multipart::Part::bytes(tarball_bytes.clone())
                    .file_name("package.tar")
                    .mime_str("application/tar")?,
            );
            let response = client
                .post(format!("{}/publish", test.url))
                .multipart(form)
                .send()
                .await?;
            if response.status().is_success() {
                assert!(false);
            }
            assert_eq!(response.text().await?, expected_error);
        }

        {
            // with neither
            let form = multipart::Form::new().part(
                "nonsense",
                multipart::Part::bytes(tarball_bytes)
                    .file_name("package.tar")
                    .mime_str("application/tar")?,
            );
            let response = client
                .post(format!("{}/publish", test.url))
                .multipart(form)
                .send()
                .await?;
            if response.status().is_success() {
                assert!(false);
            }
            assert_eq!(response.text().await?, expected_error);
        }
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_duplicate_package_hash() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login1, _password) = test.signup(None).await?;
        let (login2, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(None)?;

        let package_name = nanoid!();

        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login1.token,
            package_id: None,
            package_name,
            version_name: nanoid!(),
        };

        test.publish(Some(data.clone()), tarball.clone()).await?;

        let mut data = data;
        data.token = login2.token;

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert!(
            e.to_string()
                .starts_with("Package with hash already exists")
        );
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_duplicate_package_name() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login1, _password) = test.signup(None).await?;
        let (login2, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(Some("content1"))?;

        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login1.token,
            package_id: None,
            package_name: nanoid!(),
            version_name: nanoid!(),
        };

        test.publish(Some(data.clone()), tarball.clone()).await?;

        // reuse our data with the same package name but different hash
        let tarball = OnyxTestState::create_test_tarball(Some("content2"))?;
        let mut data = data;
        data.token = login2.token;
        data.hash = tarball.1.to_string();

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Package name is already in use!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_non_author() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login1, _password) = test.signup(None).await?;
        let (login2, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(Some("content1"))?;

        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login1.token,
            package_id: None,
            package_name: nanoid!(),
            version_name: nanoid!(),
        };

        let PublishResponse { package_id } =
            test.publish(Some(data.clone()), tarball.clone()).await?;

        let tarball = OnyxTestState::create_test_tarball(Some("content2"))?;

        let mut data = data;
        data.token = login2.token;
        data.hash = tarball.1.to_string();
        data.package_id = Some(package_id);

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Not the package author!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_name_mismatch() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(Some("content1"))?;

        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token,
            package_id: None,
            package_name: nanoid!(),
            version_name: nanoid!(),
        };

        let PublishResponse { package_id } =
            test.publish(Some(data.clone()), tarball.clone()).await?;

        let tarball = OnyxTestState::create_test_tarball(Some("content2"))?;

        let mut data = data;
        data.hash = tarball.1.to_string();
        data.package_id = Some(package_id);
        data.package_name = "incorrectname".to_string();

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Package name mismatch in publish request!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_bad_package_id() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(Some("content1"))?;

        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token,
            package_id: Some(nanoid!()),
            package_name: nanoid!(),
            version_name: nanoid!(),
        };

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Package does not exist for id!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_hash_mismatch() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(Some("content1"))?;
        let tarball2 = OnyxTestState::create_test_tarball(Some("content2"))?;

        let data = PublishData {
            hash: tarball2.1.to_string(),
            token: login.token,
            package_id: None,
            package_name: nanoid!(),
            version_name: nanoid!(),
        };

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Hash mismatch for uploaded tarball!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_duplicate_version() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(Some("content1"))?;

        let version_name = nanoid!();
        let package_name = nanoid!();
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token.clone(),
            package_id: None,
            package_name: package_name.clone(),
            version_name: version_name.clone(),
        };
        let PublishResponse { package_id } = test.publish(Some(data), tarball).await?;

        let tarball = OnyxTestState::create_test_tarball(Some("content2"))?;
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token,
            package_id: Some(package_id),
            package_name,
            version_name,
        };

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert!(
            e.to_string()
                .starts_with("Version already exists for package!")
        );
        Ok(())
    }

    #[tokio::test]
    async fn publish_package_and_new_version() -> Result<()> {
        let test = OnyxTestState::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTestState::create_test_tarball(Some("content1"))?;

        let package_name = nanoid!();
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token.clone(),
            package_id: None,
            package_name: package_name.clone(),
            version_name: nanoid!(),
        };
        let PublishResponse { package_id } = test.publish(Some(data), tarball).await?;

        let tarball = OnyxTestState::create_test_tarball(Some("content2"))?;
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token,
            package_id: Some(package_id.clone()),
            package_name,
            version_name: nanoid!(),
        };

        let r2 = test.publish(Some(data), tarball).await?;
        assert_eq!(r2.package_id, package_id);
        Ok(())
    }
}

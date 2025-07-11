use std::io::Write;

use anyhow::Result;
use axum::extract::Multipart;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use nanoid::nanoid;
use redb::ReadableTable;
use tempfile::tempfile;

use onyx_api::prelude::*;

use crate::PACKAGE_NAME_TABLE;
use crate::PACKAGE_VERSION_NAME_TABLE;
use crate::VERSION_TABLE;

use super::AUTH_TOKEN_TABLE;
use super::OnyxError;
use super::OnyxState;
use super::PACKAGE_TABLE;
use super::PACKAGE_VERSION_TABLE;
use super::timestamp;

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

    // now we're authed, and confirmed to be the author of the package
    // let's examine the provided tarball
    let mut tarball = tempfile()?;
    tarball.write_all(&tarball_data)?;

    let actual_hash = nrpm_tarball::hash(&mut tarball)?;

    if blake3::Hash::from_hex(publish_data.hash)? != actual_hash {
        println!("WARNING: hash mismatch for uploaded package, computed: {actual_hash}");
        return Err(OnyxError::bad_request(
            "Hash mismatch for uploaded tarball!",
        ));
    }

    // now write our package to the db
    let write = state.db.begin_write()?;
    let package = {
        let mut package_table = write.open_table(PACKAGE_TABLE)?;
        let mut package_version_table = write.open_multimap_table(PACKAGE_VERSION_TABLE)?;
        let mut version_table = write.open_table(VERSION_TABLE)?;
        let mut package_name_table = write.open_table(PACKAGE_NAME_TABLE)?;
        let mut package_version_name_table = write.open_table(PACKAGE_VERSION_NAME_TABLE)?;

        // generate a new version id for what is being published
        let version_id = HashId::from(actual_hash);

        let package =
            if let Some(package_id) = package_name_table.get(publish_data.package_name.as_str())? {
                // make sure we're the author of the package
                let mut package = if let Some(package) = package_table.get(package_id.value())? {
                    package.value()
                } else {
                    unreachable!("package tables are inconsistent")
                };
                if package.author_id != user_id {
                    return Err(OnyxError::bad_request(
                        "You are not authorized to publish versions of this package",
                    ));
                }
                package.latest_version_id = version_id.clone();
                package_table.insert(package_id.value(), package.clone())?;
                package
            } else {
                let package = PackageModel {
                    id: nanoid!(),
                    name: publish_data.package_name,
                    author_id: user_id.clone(),
                    latest_version_id: version_id.clone(),
                };
                package_table.insert(package.id.as_str(), package.clone())?;
                package_name_table.insert(package.name.as_str(), package.id.as_str())?;
                package
            };

        if let Some(_) = version_table.get(&version_id)? {
            return Err(OnyxError::bad_request("Package with hash already exists"));
        } else {
            if let Err(e) = state
                .storage
                .ingest_file(&mut tarball, HashId::from(actual_hash).to_string())
            {
                println!(
                    "WARNING: package already exists with hash: {} {}",
                    actual_hash.to_string(),
                    e
                );
                return Err(OnyxError::bad_request(&format!(
                    "File with hash already exists: {}",
                    actual_hash.to_string()
                )));
            }
        }

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
            version_id.clone(),
        )?;
        package_version_table.insert(package.id.as_str(), version_id.clone())?;
        version_table.insert(
            version_id.clone(),
            PackageVersionModel {
                id: version_id,
                name: publish_data.version_name,
                author_id: user_id,
                package_id: package.id.clone(),
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
        let test = OnyxTest::new().await?;
        reqwest::Client::new().get(&test.url).send().await?;
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_without_token() -> Result<()> {
        let test = OnyxTest::new().await?;
        let tarball = OnyxTest::create_test_tarball(None)?;

        let mut publish_data = PublishData::default();
        publish_data.hash = tarball.1.to_string();
        let e = test.publish(Some(publish_data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Publish request contains invalid token!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_expired_token() -> Result<()> {
        let test = OnyxTest::new().await?;
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

        let (tarball_bytes, hash) = OnyxTest::create_test_tarball(None)?;
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
        let test = OnyxTest::new().await?;
        let (tarball_bytes, _hash) = OnyxTest::create_test_tarball(None)?;

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
        let test = OnyxTest::new().await?;
        let (tarball_bytes, hash) = OnyxTest::create_test_tarball(None)?;
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
        let test = OnyxTest::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTest::create_test_tarball(None)?;

        let package_name = nanoid!();

        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token,
            package_name,
            version_name: nanoid!(),
        };

        let PublishResponse { package_id: _ } =
            test.publish(Some(data.clone()), tarball.clone()).await?;

        let mut data = data;
        data.version_name = nanoid!();

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        println!("{e}");
        assert!(
            e.to_string()
                .starts_with("Package with hash already exists")
        );
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_non_author() -> Result<()> {
        let test = OnyxTest::new().await?;
        let (login1, _password) = test.signup(None).await?;
        let (login2, _password) = test.signup(None).await?;
        let tarball = OnyxTest::create_test_tarball(Some("content1"))?;

        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login1.token,
            package_name: nanoid!(),
            version_name: nanoid!(),
        };

        let PublishResponse { package_id: _ } =
            test.publish(Some(data.clone()), tarball.clone()).await?;

        let tarball = OnyxTest::create_test_tarball(Some("content2"))?;

        let mut data = data;
        data.token = login2.token;
        data.hash = tarball.1.to_string();

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert_eq!(
            e.to_string(),
            "You are not authorized to publish versions of this package"
        );
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_hash_mismatch() -> Result<()> {
        let test = OnyxTest::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTest::create_test_tarball(Some("content1"))?;
        let tarball2 = OnyxTest::create_test_tarball(Some("content2"))?;

        let data = PublishData {
            hash: tarball2.1.to_string(),
            token: login.token,
            package_name: nanoid!(),
            version_name: nanoid!(),
        };

        let e = test.publish(Some(data), tarball).await.unwrap_err();
        assert_eq!(e.to_string(), "Hash mismatch for uploaded tarball!");
        Ok(())
    }

    #[tokio::test]
    async fn fail_publish_duplicate_version_name() -> Result<()> {
        let test = OnyxTest::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTest::create_test_tarball(Some("content1"))?;

        let version_name = nanoid!();
        let package_name = nanoid!();
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token.clone(),
            package_name: package_name.clone(),
            version_name: version_name.clone(),
        };
        let PublishResponse { package_id: _ } = test.publish(Some(data), tarball).await?;

        let tarball = OnyxTest::create_test_tarball(Some("content2"))?;
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token,
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
        let test = OnyxTest::new().await?;
        let (login, _password) = test.signup(None).await?;
        let tarball = OnyxTest::create_test_tarball(Some("content1"))?;

        let package_name = nanoid!();
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token.clone(),
            package_name: package_name.clone(),
            version_name: nanoid!(),
        };
        let PublishResponse { package_id } = test.publish(Some(data), tarball).await?;

        let tarball = OnyxTest::create_test_tarball(Some("content2"))?;
        let data = PublishData {
            hash: tarball.1.to_string(),
            token: login.token,
            package_name,
            version_name: nanoid!(),
        };

        let r2 = test.publish(Some(data), tarball).await?;
        assert_eq!(r2.package_id, package_id);
        Ok(())
    }
}

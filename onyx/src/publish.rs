use std::fs;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

use anyhow::Result;
use axum::extract::Multipart;
use axum::extract::State;
use db::PackageModel;
use db::PackageVersionModel;
use nanoid::nanoid;
use redb::ReadableTable;
use serde::Deserialize;
use serde::Serialize;
use tempfile::tempfile;

use crate::PACKAGE_NAME_TABLE;

use super::AUTH_TOKEN_TABLE;
use super::OnyxError;
use super::OnyxState;
use super::PACKAGE_TABLE;
use super::PACKAGE_VERSION_TABLE;
use super::STORAGE_PATH;
use super::timestamp;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PublishData {
    pub hash: String,
    pub token: String,
    pub package_id: Option<String>, // None if creating a new package
    pub package_name: String,
    pub version_name: String,
}

pub async fn publish(
    State(state): State<OnyxState>,
    mut multipart: Multipart,
) -> Result<(), OnyxError> {
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
            "Publish request contained invalid token!",
        ));
    };
    if let Some(package_id) = &publish_data.package_id {
        if let Some(package) = package_table.get(package_id.as_str())? {
            let package = package.value();
            if package.author_id != user_id {
                return Err(OnyxError::bad_request("Not authorized!"));
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
    tarball.seek(SeekFrom::Start(0))?;
    let actual_hash = common::hash_tarball(&tarball)?;

    if blake3::Hash::from_hex(publish_data.hash)? != actual_hash {
        println!("WARNING: hash mismatch for uploaded package");
        return Err(OnyxError::bad_request(
            "Hash mismatch for uploaded tarball!",
        ));
    }

    let storage_path = std::env::current_dir()?.join(STORAGE_PATH);
    let target_path = storage_path.join(format!("{}.tar", actual_hash.to_string()));
    if target_path.exists() {
        println!(
            "WARNING: package already exists with hash: {}",
            actual_hash.to_string()
        );
        return Err(OnyxError::bad_request(&format!(
            "Package with hash {} already exists!",
            actual_hash.to_string()
        )));
    }
    fs::write(target_path, tarball_data)?;

    // now write our package to the db
    let write = state.db.begin_write()?;
    {
        let mut package_table = write.open_table(PACKAGE_TABLE)?;
        let mut version_table = write.open_table(PACKAGE_VERSION_TABLE)?;
        let mut package_name_table = write.open_table(PACKAGE_NAME_TABLE)?;

        // do the name availability check here to avoid a race condition
        // (check for name before starting write, another threads takes name, this thread overwrites name)
        if publish_data.package_id.is_none() {
            // creating a new package, verify that name is available
            if let Some(_) = package_name_table.get(publish_data.package_name.as_str())? {
                return Err(OnyxError::bad_request("Package name is in use!"));
            }
        }
        package_name_table.insert(publish_data.package_name.as_str(), ())?;

        // generate a new version id for what is being published
        let version_id = nanoid!();

        let package = if let Some(package_id) = publish_data.package_id {
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

        version_table.insert(
            version_id.as_str(),
            PackageVersionModel {
                id: version_id.clone(),
                name: publish_data.version_name,
                author_id: user_id,
                package_id: package.id,
                hash: *actual_hash.as_bytes(),
                created_at: timestamp(),
            },
        )?;
    }
    write.commit()?;

    Ok(())
}

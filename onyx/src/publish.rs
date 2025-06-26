use std::fs;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;

use anyhow::Result;
use axum::extract::Multipart;
use axum::extract::State;
use tempfile::tempfile;

use crate::timestamp;

use super::AUTH_TOKEN_TABLE;
use super::OnyxError;
use super::OnyxState;
use super::STORAGE_PATH;

pub async fn publish(
    State(state): State<OnyxState>,
    mut multipart: Multipart,
) -> Result<(), OnyxError> {
    let mut expected_hash = None;
    let mut tarball_data = None;
    let mut token = None;
    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().ok_or(OnyxError::bad_request(
            "All fields in multipart upload must have names",
        ))?;
        match name {
            "tarball" => {
                let data = field.bytes().await?;
                tarball_data = Some(data);
            }
            "hash" => {
                let hash_text = field.text().await?;
                expected_hash = Some(
                    blake3::Hash::from_hex(&hash_text)
                        .map_err(|_| OnyxError::bad_request("Error decoding hash hex value"))?,
                );
            }
            "token" => {
                token = Some(field.text().await?);
            }
            _ => {}
        }
    }
    // Verify we got all required fields
    let (expected_hash, tarball_data, token) = match (expected_hash, tarball_data, token) {
        (Some(e), Some(t), Some(tt)) => (e, t, tt),
        _ => {
            return Err(OnyxError::bad_request(
                "Publish request missing field, expected: \"hash\", \"tarball\", \"token\"",
            ));
        }
    };
    let read = state.db.begin_read()?;
    let auth_table = read.open_table(AUTH_TOKEN_TABLE)?;
    if let Some(entry) = auth_table.get(token.as_str())? {
        let (_user_id, expires_at) = entry.value();
        if timestamp() > expires_at {
            return Err(OnyxError::bad_request(
                "Publish request contained invalid token!",
            ));
        }
    } else {
        return Err(OnyxError::bad_request(
            "Publish request contained invalid token!",
        ));
    }

    let mut tarball = tempfile()?;
    tarball.write_all(&tarball_data)?;
    tarball.seek(SeekFrom::Start(0))?;
    let actual_hash = common::hash_tarball(&tarball)?;

    if expected_hash != actual_hash {
        println!("WARNING: hash mismatch for uploaded package");
        return Err(OnyxError::bad_request(
            "Hash mismatch for uploaded tarball!",
        ));
    }
    // otherwise write our tarball to file
    let storage_path = std::env::current_dir()?.join(STORAGE_PATH);
    let target_path = storage_path.join(format!("{}.tar", expected_hash.to_string()));
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
    Ok(())
}

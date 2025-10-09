use anyhow::Result;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use onyx_api::prelude::*;
use redb::ReadableTable;

use crate::VERSION_TABLE;

use super::OnyxError;
use super::OnyxState;
use super::PACKAGE_TABLE;

pub async fn load_package_version(
    State(state): State<OnyxState>,
    Path(name): Path<String>,
) -> Result<ResponseJson<(PackageModel, PackageVersionModel)>, OnyxError> {
    let read = state.db.begin_read()?;
    let package_name_table = read.open_table(PACKAGE_NAME_TABLE)?;
    let package_id = package_name_table
        .get(name.as_str())?
        .ok_or(OnyxError::bad_request(&format!(
            "Unable to find package \"{}\"",
            name,
        )))?;
    let package_table = read.open_table(PACKAGE_TABLE)?;
    let package = package_table
        .get(package_id.value())?
        .ok_or(OnyxError::bad_request(&format!(
            "Unable to find package for id \"{}\"",
            package_id.value(),
        )))?
        .value();
    let version_table = read.open_table(VERSION_TABLE)?;
    let version = version_table
        .get(&package.latest_version_id)?
        .ok_or(OnyxError::bad_request(&format!(
            "Unable to find version for id \"{}\"",
            package.latest_version_id.to_string(),
        )))?
        .value();
    Ok(ResponseJson((package, version)))
}

pub async fn list_packages(
    State(state): State<OnyxState>,
) -> Result<ResponseJson<Vec<(PackageModel, PackageVersionModel)>>, OnyxError> {
    let read = state.db.begin_read()?;
    let package_table = read.open_table(PACKAGE_TABLE)?;
    let version_table = read.open_table(VERSION_TABLE)?;
    let mut out = vec![];
    for result in package_table.iter()? {
        let (_id, package) = result?;
        if let Some(latest_version) = version_table.get(package.value().latest_version_id)? {
            out.push((package.value(), latest_version.value()));
        } else {
            log::warn!(
                "failed to load latest version for package {}",
                package.value().name
            );
        }
    }
    Ok(ResponseJson(out))
}

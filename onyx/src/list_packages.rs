use anyhow::Result;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use onyx_api::prelude::*;
use redb::ReadableTable;

use crate::VERSION_TABLE;

use super::OnyxError;
use super::OnyxState;
use super::PACKAGE_TABLE;

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
            println!(
                "WARNING: failed to load latest version for package: {}",
                package.value().name
            );
        }
    }
    Ok(ResponseJson(out))
}

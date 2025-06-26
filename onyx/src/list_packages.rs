use anyhow::Result;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use db::PackageModel;
use redb::ReadableTable;

use super::OnyxError;
use super::OnyxState;
use super::PACKAGE_TABLE;

pub async fn list_packages(
    State(state): State<OnyxState>,
) -> Result<ResponseJson<Vec<PackageModel>>, OnyxError> {
    let read = state.db.begin_read()?;
    let package_table = read.open_table(PACKAGE_TABLE)?;
    let mut out = vec![];
    for result in package_table.iter()? {
        let (_id, package) = result?;
        out.push(package.value());
    }
    Ok(ResponseJson(out))
}

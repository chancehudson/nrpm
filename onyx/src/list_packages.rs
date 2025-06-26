use anyhow::Result;
use axum::extract::State;
use axum::response::Json as ResponseJson;
use db::PackageModel;
use redb::ReadableTable;

use crate::{OnyxError, PACKAGE_TABLE};

use super::OnyxState;

pub async fn list_packages(
    State(state): State<OnyxState>,
) -> Result<ResponseJson<Vec<PackageModel>>, OnyxError> {
    let read = state.db.begin_write()?;
    let package_table = read.open_table(PACKAGE_TABLE)?;
    let mut out = vec![];
    for result in package_table.iter()? {
        let (_id, package) = result?;
        out.push(package.value());
    }
    Ok(ResponseJson(out))
}

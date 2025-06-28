use std::str::FromStr;

use anyhow::Result;
use axum::body::Body;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Response;
use onyx_api::db::HashId;
use onyx_api::db::PACKAGE_TABLE;
use onyx_api::db::VERSION_TABLE;
use tokio_util::io::ReaderStream;

use super::OnyxError;
use super::OnyxState;

pub async fn download_package(
    State(state): State<OnyxState>,
    Path(id): Path<String>,
) -> Result<Response, OnyxError> {
    let reader = state.storage.reader_async(&id).await?;

    let stream = ReaderStream::new(reader);
    let body = Body::from_stream(stream);

    let read = state.db.begin_read()?;
    let package_tree = read.open_table(PACKAGE_TABLE)?;
    let version_tree = read.open_table(VERSION_TABLE)?;
    if let Some(version) = version_tree.get(HashId::from_str(&id)?)? {
        let version = version.value();
        if let Some(package) = package_tree.get(version.package_id.as_str())? {
            let package = package.value();
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "application/octet-stream"
                    .parse()
                    .map_err(|_| OnyxError::default())?,
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}_{}.tar\"",
                    package.name, version.name
                )
                .parse()
                .map_err(|_| OnyxError::default())?,
            );

            Ok((headers, body).into_response())
        } else {
            Err(OnyxError::bad_request("Unable to find package"))
        }
    } else {
        Err(OnyxError::bad_request("Unable to find version"))
    }
}

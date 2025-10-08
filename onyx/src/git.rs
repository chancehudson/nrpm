use anyhow::Result;
use axum::body::Body;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Response;
use nrpm_tarball::ptk_bytes;
use onyx_api::db::PackageModel;
use reqwest::StatusCode;
use tokio::io::AsyncReadExt;

use super::OnyxError;
use super::OnyxState;

pub async fn empty() -> Result<Response, OnyxError> {
    let mut res = Response::new("not found".into());
    *res.status_mut() = StatusCode::NOT_FOUND;
    Ok(res)
}

pub async fn mocked_refs(
    State(state): State<OnyxState>,
    Path(package_name): Path<String>,
) -> Result<Response, OnyxError> {
    if let Some(_version) = PackageModel::latest_version(state.db, &package_name)? {
        let mut res = Response::new(
            [
                ptk_bytes("version 2\n"),
                ptk_bytes("agent=onyx/0.0.0-pre-release\n"),
                ptk_bytes("ls-refs=unborn\n"),
                ptk_bytes("ls-refs=symrefs\n"),
                ptk_bytes("fetch=shallow\n"),
                "0000".into(),
            ]
            .concat()
            .into(),
        );
        res.headers_mut().insert(
            "Content-Type",
            "application/x-git-upload-pack-advertisement"
                .parse()
                .unwrap(),
        );
        res.headers_mut()
            .insert("Cache-Control", "no-cache".parse().unwrap());
        Ok(res)
    } else {
        let mut res = Response::new("not found".into());
        *res.status_mut() = StatusCode::NOT_FOUND;
        Ok(res)
    }
}

/// Handles loading references and sending packs
pub async fn mocked_upload_pack(
    State(state): State<OnyxState>,
    Path(package_name): Path<String>,
    body: String,
) -> Result<Response, OnyxError> {
    if let Some(version) = PackageModel::latest_version(state.db, &package_name)? {
        let mut res = Response::new(Body::empty());
        res.headers_mut().insert(
            "Content-Type",
            "application/x-git-upload-pack-result".parse().unwrap(),
        );
        res.headers_mut()
            .insert("Cache-Control", "no-cache".parse().unwrap());

        if body.contains("0014command=ls-refs") {
            let mut refs = state
                .storage
                .reader_async(
                    &version.id.to_string(),
                    onyx_api::prelude::FileType::GitRefs,
                )
                .await?;
            let mut refs_bytes = Vec::default();
            refs.read_to_end(&mut refs_bytes).await?;
            *res.body_mut() = refs_bytes.into();
        } else {
            let mut pack = state
                .storage
                .reader_async(
                    &version.id.to_string(),
                    onyx_api::prelude::FileType::GitPack,
                )
                .await?;
            let mut res_bytes = vec![
                ptk_bytes("packfile\n"),
                ptk_bytes(&format!(
                    "\x02🚒 nrpm downloading {}@{}\n",
                    package_name, version.name
                )),
            ];
            let mut pack_bytes = Vec::default();
            pack.read_to_end(&mut pack_bytes).await?;
            for chunk in pack_bytes.chunks((pack_bytes.len() / (10 * 1024)).max(1)) {
                // manually calculate the length prefixes
                let bytes = ["\x01".as_bytes(), chunk].concat();
                res_bytes.push(format!("{:04x}", 4 + bytes.len()).into_bytes());
                res_bytes.push(bytes);
            }

            res_bytes.push("0000".into());
            *res.body_mut() = res_bytes.concat().into();
        }

        Ok(res)
    } else {
        let mut res = Response::new("not found".into());
        *res.status_mut() = StatusCode::NOT_FOUND;
        Ok(res)
    }
}

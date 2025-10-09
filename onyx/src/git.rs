use std::sync::LazyLock;

use anyhow::Result;
use axum::body::Body;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Response;
use nrpm_tarball::ptk_bytes;
use onyx_api::db::GIT_PACK_TABLE;
use onyx_api::db::GIT_REFS_TABLE;
use onyx_api::db::PackageModel;
use regex::Regex;
use reqwest::StatusCode;

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
    if let Some(package) = PackageModel::package_by_name(state.db.clone(), &package_name)? {
        let mut res = Response::new(Body::empty());
        res.headers_mut().insert(
            "Content-Type",
            "application/x-git-upload-pack-result".parse().unwrap(),
        );
        res.headers_mut()
            .insert("Cache-Control", "no-cache".parse().unwrap());

        log::debug!("upload-pack: {}", body);

        if body.contains("0014command=ls-refs") {
            let read = state.db.begin_read()?;
            let git_refs_table = read.open_table(GIT_REFS_TABLE)?;
            // a list of refs, we'll manually add a terminating sequence
            let refs = git_refs_table
                .get(package.id.as_str())?
                .and_then(|v| Some(v.value().to_string()))
                .unwrap_or_default();

            *res.body_mut() = format!("{}0000", refs).into_bytes().into();
        } else if body.contains("0011command=fetch") {
            // parse what commit is being requested, then send the pack data for that commit
            static COMMIT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r"0032want ([a-f0-9]{40})").expect("failed to create commit regex")
            });
            let commit_hex = if let Some(caps) = COMMIT_REGEX.captures(&body)
                // first entry is full match, we want the subgroup
                && caps.len() >= 2
            {
                caps[1].to_string()
            } else {
                return Err(OnyxError::bad_request("unable to find want commits"));
            };

            let read = state.db.begin_read()?;
            let git_packs_table = read.open_table(GIT_PACK_TABLE)?;
            let pack_bytes = if let Some(pack) = git_packs_table.get(commit_hex.as_str())? {
                pack.value()
            } else {
                return Err(OnyxError::bad_request(&format!(
                    "unable to find pack for commit {}",
                    commit_hex
                )));
            };

            // determine the name of the ref for the download message
            // TODO: consider storing this in the db
            let git_refs_table = read.open_table(GIT_REFS_TABLE)?;
            // a list of refs, we'll manually add a terminating sequence
            let refs = git_refs_table
                .get(package.id.as_str())?
                .and_then(|v| Some(v.value().to_string()))
                .unwrap_or_default();

            let ref_regex = Regex::new(&format!("{} refs/heads/(.*)", commit_hex))
                .expect("failed to build ref_regex");
            let version_name = if let Some(caps) = ref_regex.captures(&refs)
                && caps.len() >= 2
            {
                caps[1].to_string()
            } else {
                "unknown_version".to_string()
            };

            let mut res_bytes = vec![
                ptk_bytes("packfile\n"),
                ptk_bytes(&format!(
                    "\x02🚒 nrpm downloading {}@{}\n",
                    package_name, version_name
                )),
            ];
            for chunk in pack_bytes.chunks((pack_bytes.len() / (10 * 1024)).max(1)) {
                // manually calculate the length prefixes
                let bytes = ["\x01".as_bytes(), chunk].concat();
                res_bytes.push(format!("{:04x}", 4 + bytes.len()).into_bytes());
                res_bytes.push(bytes);
            }

            res_bytes.push("0000".into());
            *res.body_mut() = res_bytes.concat().into();
        } else {
            return Err(OnyxError::bad_request("unknown git command"));
        }

        Ok(res)
    } else {
        let mut res = Response::new("not found".into());
        *res.status_mut() = StatusCode::NOT_FOUND;
        Ok(res)
    }
}

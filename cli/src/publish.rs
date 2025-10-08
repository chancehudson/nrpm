use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use dialoguer::Input;
use onyx_api::prelude::*;
use tempfile::tempfile;

pub async fn upload_tarball(
    api: &OnyxApi,
    pkg_dir: &Path,
    archive_path: Option<PathBuf>,
) -> Result<()> {
    println!("📦 Packaging {:?}", pkg_dir);
    if let Ok(metadata) = std::fs::metadata(pkg_dir) {
        if !metadata.is_dir() {
            anyhow::bail!("Path is not a directory: {:?}", pkg_dir);
        }
    } else {
        anyhow::bail!("Unable to stat path: {:?}", pkg_dir);
    }
    let mut tarball = nrpm_tarball::create(pkg_dir, tempfile()?)?;
    if let Some(path) = archive_path {
        std::io::copy(&mut tarball, &mut File::create(path)?)?;
        return Ok(());
    }
    println!("🔃 Redirecting to authorize");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let login = super::attempt_auth().await?;

    let hash = nrpm_tarball::hash(&mut tarball)?;
    // reset the file handle for copying to final destination
    tarball.seek(std::io::SeekFrom::Start(0))?;
    let mut tarball_bytes = vec![];
    tarball.read_to_end(&mut tarball_bytes)?;
    println!("Uploading: {} bytes", tarball_bytes.len());
    println!("Hash: {}", hash.to_string());

    let package_name: String = Input::new().with_prompt("Package name").interact_text()?;
    let version_name: String = Input::new().with_prompt("Version name").interact_text()?;

    match api
        .publish(
            PublishData {
                hash: hash.to_string(),
                token: login.token,
                package_name: package_name.clone(),
                version_name: version_name.clone(),
            },
            tarball_bytes,
        )
        .await
    {
        Ok(PublishResponse { package_id }) => {
            println!(
                "Success: published version \"{version_name}\" for package \"{package_name}\""
            );
            println!("Package id: {package_id}");
        }
        Err(e) => {
            println!("ERROR: failed to publish package");
            println!("{e}");
        }
    }
    Ok(())
}

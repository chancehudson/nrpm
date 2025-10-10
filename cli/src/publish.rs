use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use onyx_api::prelude::*;
use tempfile::tempfile;

use nargo_parse::*;

pub async fn upload_tarball(
    api: &OnyxApi,
    pkg_dir: &Path,
    archive_path: Option<PathBuf>,
) -> Result<()> {
    log::info!("📦 Packaging {:?}", pkg_dir);
    if let Ok(metadata) = std::fs::metadata(pkg_dir) {
        if !metadata.is_dir() {
            anyhow::bail!("Path is not a directory: {:?}", pkg_dir);
        }
    } else {
        anyhow::bail!("Unable to stat path: {:?}", pkg_dir);
    }
    let config =
        NargoConfig::load(pkg_dir).with_context(|| "Nargo.toml not found in directory!")?;
    config.validate_metadata()?;
    let version_name = config.package.version.ok_or(anyhow::anyhow!(
        "no version field in Nargo.toml package section"
    ))?;
    let package_name = config.package.name;

    let mut tarball = nrpm_tarball::create(pkg_dir, tempfile()?)?;
    if let Some(path) = archive_path {
        std::io::copy(&mut tarball, &mut File::create(path)?)?;
        return Ok(());
    }
    let hash = nrpm_tarball::hash_tarball(&mut tarball)?;

    println!("🔃 Redirecting to authorize");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let login = super::attempt_auth().await?;

    println!(""); // line break
    if !dialoguer::Confirm::new()
        .with_prompt(format!(
            "Publish \"{package_name}\" version \"{version_name}\"?"
        ))
        .interact()?
    {
        println!("User cancelled the action");
        return Ok(());
    }

    // reset the file handle for copying to final destination
    tarball.seek(std::io::SeekFrom::Start(0))?;
    let mut tarball_bytes = vec![];
    tarball.read_to_end(&mut tarball_bytes)?;
    println!("Uploading: {} bytes", tarball_bytes.len());
    println!("Hash: {}", hash.to_string());
    match api
        .publish(
            PublishData {
                hash: hash.to_string(),
                token: login.token,
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
            eprintln!("failed to publish package");
            eprintln!("{e:?}");
        }
    }
    Ok(())
}

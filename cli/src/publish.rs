use std::fs::File;
use std::io::Read;
use std::io::Seek;

use anyhow::Result;
use dialoguer::Input;
use onyx_api::prelude::*;

pub async fn upload_tarball(login: LoginResponse, api: &OnyxApi, tarball: &mut File) -> Result<()> {
    let hash = nrpm_tarball::hash(tarball)?;
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
                package_id: None,
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

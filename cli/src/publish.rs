use std::fs::File;
use std::io::Read;
use std::io::Seek;

use anyhow::Result;
use reqwest::multipart;

pub async fn upload_tarball(url: &str, tarball: File) -> Result<()> {
    let mut tarball = tarball;
    let client = reqwest::Client::new();
    let hash = common::hash_tarball(&tarball)?;
    // reset the file handle for copying to final destination
    tarball.seek(std::io::SeekFrom::Start(0))?;
    let mut tarball_bytes = vec![];
    tarball.read_to_end(&mut tarball_bytes)?;
    println!("Uploading: {} bytes", tarball_bytes.len());
    println!("Hash: {}", hash.to_string());
    let form = multipart::Form::new().text("hash", hash.to_string()).part(
        "tarball",
        multipart::Part::bytes(tarball_bytes)
            .file_name("package.tar")
            .mime_str("application/tar")?,
    );
    let response = client.post(url).multipart(form).send().await?;
    if response.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("Upload failed with status: {}", response.status());
    }
}

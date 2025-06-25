use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::path::PathBuf;

use anyhow::Result;
use ignore::WalkBuilder;
use reqwest::multipart;
use tempfile::tempfile;

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

/// Create a tarball from `path`, which must exist and be a directory. Returned value with be
/// a temporary File handle that is removed on Drop. Make sure to copy the file if persistence is needed!
///
/// This function will look for a .gitignore in all directories and follow it.
/// Empty directories are not included. Irregular files (symlinks, block devices, etc) are not included.
/// File permission errors will cause a failure. File paths are stored relative to `path`.
pub fn create_tarball(path: PathBuf) -> Result<File> {
    let path = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => anyhow::bail!("Failed to canonicalize path: {:?} error: {:?}", path, e),
    };
    if !path.exists() {
        anyhow::bail!("Path does not exist: {:?}", path);
    }
    if !path.is_dir() {
        anyhow::bail!("Path is not a directory: {:?}", path);
    }
    let tar_file = tempfile()?;
    let mut a = tar::Builder::new(tar_file);
    let walker = WalkBuilder::new(&path)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .hidden(false) // include hidden files
        .filter_entry(|entry| {
            // Exclude .git directories
            !(entry.file_name() == ".git" && entry.file_type().map_or(false, |ft| ft.is_dir()))
        })
        .build();

    for entry in walker {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            // empty directories will not be included
            continue;
        }
        if !entry_path.is_file() {
            println!("WARNING: skipping irregular file {:?}", entry_path);
            continue;
        }
        let relative_path = entry_path.strip_prefix(&path)?;
        let mut file = match File::open(entry_path) {
            Ok(f) => f,
            Err(e) => anyhow::bail!(
                "Failed to open file at path: {:?}, error: {:?}",
                entry_path,
                e
            ),
        };
        a.append_file(relative_path, &mut file)?;
    }
    let mut tarball = a.into_inner()?;
    // reset the file handle for use by caller
    tarball.seek(std::io::SeekFrom::Start(0))?;
    Ok(tarball)
}

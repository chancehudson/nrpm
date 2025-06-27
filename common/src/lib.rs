use std::fs::File;
use std::io::Seek;
use std::path::PathBuf;

use anyhow::Result;
use ignore::WalkBuilder;
use tempfile::tempfile;

mod checksum;

pub use checksum::hash_tarball;

pub fn timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

/// Create a tarball from `path`, which must exist and be a directory. Returned value with be
/// a temporary File handle that is removed on Drop. Make sure to copy the file if persistence is needed!
///
/// This function will look for a .gitignore in all directories and follow it.
/// Empty directories are not included. Irregular files (symlinks, block devices, etc) are not included.
/// File permission errors will cause a failure. File paths are stored relative to `path`.
pub fn create_tarball(path: PathBuf, tar_file: File) -> Result<File> {
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
    let mut archive = tar::Builder::new(tar_file);
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
        archive.append_file(relative_path, &mut file)?;
    }
    archive.finish()?;
    let mut tarball = archive.into_inner()?;
    // reset the file handle for use by caller
    tarball.seek(std::io::SeekFrom::Start(0))?;
    Ok(tarball)
}

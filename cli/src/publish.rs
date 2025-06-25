use std::collections::BTreeMap;
use std::fs::File;
use std::io::Seek;
use std::path::Component;
use std::path::PathBuf;

use anyhow::Result;
use ignore::WalkBuilder;
use tar::Archive;
use tar::EntryType;
use tempfile::tempfile;

/// Create a tarball from `path`, which must exist and be a directory.
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
    // reset the file handle for copying to final destination
    tarball.seek(std::io::SeekFrom::Start(0))?;
    Ok(tarball)
}

/// Take a tar archive and calculate a content based hash. Each file is separately hashed
/// by hashing each path component followed by contents. A final hash is created by combining
/// all file hashes in lexicographic order of file paths.
pub fn hash_tarball(tarball: &File) -> Result<blake3::Hash> {
    let mut archive = Archive::new(tarball);

    println!("Hashing files...");
    // this approach allows content hashes to be calculated in parallel
    // while remaining deterministic
    let mut ordered_files: BTreeMap<PathBuf, blake3::Hash> = BTreeMap::new();
    for entry in archive.entries()? {
        let entry = entry?;
        match entry.header().entry_type() {
            EntryType::Regular => {
                let mut hasher = blake3::Hasher::new();
                // only hash the filepath and the contents
                let path = entry.path()?.to_path_buf();
                for component in path.components() {
                    match component {
                        Component::Normal(component) => {
                            hasher.update(component.as_encoded_bytes());
                        }
                        _ => anyhow::bail!("Non-normal path component detected in tarball"),
                    }
                }
                hasher.update_reader(entry)?;
                ordered_files.insert(path, hasher.finalize());
            }
            EntryType::Directory => {
                continue;
            }
            _ => anyhow::bail!(
                "Irregular entry detected in tar archive. Only directories and files are allowed in package tarballs!"
            ),
        }
    }
    // now combine our ordered hashes into a final hash
    let mut hasher = blake3::Hasher::new();
    for (file, hash) in ordered_files {
        println!("{:?}", file);
        hasher.update(hash.as_bytes());
    }
    Ok(hasher.finalize())
}

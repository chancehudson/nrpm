use std::collections::BTreeMap;
use std::fs::File;
use std::path::Component;
use std::path::PathBuf;

use anyhow::Result;
use tar::Archive;
use tar::EntryType;

/// Take a tar archive and calculate a content based hash. Each file is separately hashed
/// by hashing each path component followed by contents. A final hash is created by combining
/// all file hashes in lexicographic order of file paths.
pub fn hash_tarball(tarball: &File) -> Result<blake3::Hash> {
    let mut archive = Archive::new(tarball);

    // println!("Hashing files...");
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
    for (_file, hash) in ordered_files {
        // println!("{:?}", file);
        hasher.update(hash.as_bytes());
    }
    Ok(hasher.finalize())
}

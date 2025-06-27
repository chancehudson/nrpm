use std::collections::BTreeMap;
use std::io::Read;
use std::path::Component;
use std::path::PathBuf;

use anyhow::Result;
use tar::Archive;
use tar::EntryType;

/// Take a tar archive and calculate a content based hash. Each file is separately hashed
/// by hashing each path component followed by contents. A final hash is created by combining
/// all file hashes in lexicographic order of file paths.
pub fn hash_tarball<R>(tarball: R) -> Result<blake3::Hash>
where
    R: Read,
{
    let mut archive = Archive::new(tarball);

    // println!("Hashing files...");
    // this approach allows content hashes to be calculated in parallel
    // while remaining deterministic
    let mut ordered_files: BTreeMap<PathBuf, blake3::Hash> = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        match entry.header().entry_type() {
            EntryType::Regular => {
                let mut hasher = blake3::Hasher::new();
                // only hash the filepath and the contents
                let path = entry.path()?.to_path_buf();
                for component in path.components() {
                    match component {
                        Component::Normal(component) => {
                            println!("{}", component.to_string_lossy());
                            hasher.update(component.as_encoded_bytes());
                        }
                        _ => anyhow::bail!("Non-normal path component detected in tarball"),
                    }
                }
                let mut str = String::new();
                entry.read_to_string(&mut str)?;
                println!("content: {}", str);
                hasher.update_reader(str.as_bytes())?;
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

#[cfg(test)]
mod tests {
    fn generate_hash() {
        // let workdir = tempfile::TempDir::new()?;
        // fs::write(workdir.path().join("aaaaa"), "testcontents\n")?;
        // let tarball = create_tarball(workdir.path().to_path_buf())?;
        // let mut tarball_clone = tarball.try_clone()?;
        // let hash = hash_tarball(&tarball)?;
    }
}

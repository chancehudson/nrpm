use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Result;
use ignore::WalkBuilder;
use tar::Archive;
use tar::EntryType;

/// Take a tar archive and calculate a content based hash. Each file is separately hashed
/// by hashing each path component followed by contents. A final hash is created by combining
/// all file hashes in lexicographic order of file paths.
pub fn hash(tarball: &mut File) -> Result<blake3::Hash> {
    tarball.seek(SeekFrom::Start(0))?;
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
                            // println!("{}", component.to_string_lossy());
                            hasher.update(component.as_encoded_bytes());
                        }
                        _ => anyhow::bail!("Non-normal path component detected in tarball"),
                    }
                }
                let mut str = String::new();
                entry.read_to_string(&mut str)?;
                // println!("content: {}", str);
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

/// Create a tarball from `path`, which must exist and be a directory. Returned value with be
/// a temporary File handle that is removed on Drop. Make sure to copy the file if persistence is needed!
///
/// This function will look for a .gitignore in all directories and follow it.
/// Empty directories are not included. Irregular files (symlinks, block devices, etc) are not included.
/// File permission errors will cause a failure. File paths are stored relative to `path`.
pub fn create(path: &Path, tar_file: File) -> Result<File> {
    // will detect non-existent paths
    let path = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => anyhow::bail!("Failed to canonicalize path: {:?} error: {:?}", path, e),
    };
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn should_return_start_of_tarball() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;

        let test_file = tempdir.path().join("test.txt");
        fs::write(test_file, "test")?;

        let mut tarball = create(tempdir.path(), tar_file)?;

        assert_eq!(tarball.stream_position()?, 0);

        Ok(())
    }

    #[test]
    fn should_use_local_gitignore() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;

        // so the gitignore is read
        let git_dir = tempdir.path().join(".git");
        fs::create_dir(&git_dir)?;

        let gitignore = tempdir.path().join(".gitignore");
        fs::write(gitignore, "ignored.txt")?;

        let ignored_file = tempdir.path().join("ignored.txt");
        fs::write(ignored_file, "test")?;

        let test_file = tempdir.path().join("test.txt");
        fs::write(test_file, "test")?;

        let tarball = create(tempdir.path(), tar_file)?;

        let mut archive = Archive::new(tarball);

        let mut found_files = Vec::new();
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?.to_path_buf();
            found_files.push(path);
        }

        assert_eq!(found_files.len(), 2);

        assert!(found_files.contains(&"test.txt".into()));
        assert!(found_files.contains(&".gitignore".into()));
        assert!(!found_files.contains(&"ignored.txt".into()));

        Ok(())
    }

    #[test]
    fn should_use_parent_gitignore() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;

        // we'll make a .gitignore at the root tempdir, then make a child directory that will be
        // put in the tarball, which should respect the parent dir .gitignore
        let content_dir = tempdir.path().join("contents");
        fs::create_dir(&content_dir)?;

        // so the gitignore is read
        let git_dir = tempdir.path().join(".git");
        fs::create_dir(&git_dir)?;

        let gitignore = tempdir.path().join(".gitignore");
        fs::write(gitignore, "ignored.txt")?;

        let ignored_file = content_dir.join("ignored.txt");
        fs::write(ignored_file, "test")?;

        let test_file = content_dir.join("test.txt");
        fs::write(test_file, "test")?;

        let tarball = create(&content_dir, tar_file)?;

        let mut archive = Archive::new(tarball);

        let mut found_files = Vec::new();
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?.to_path_buf();
            found_files.push(path);
        }

        assert_eq!(found_files.len(), 1);
        assert_eq!(found_files[0], PathBuf::from("test.txt"));

        Ok(())
    }

    #[test]
    fn should_include_hidden_files() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;

        let test_file = tempdir.path().join(".hidden.txt");
        fs::write(test_file, "test")?;

        let tarball = create(tempdir.path(), tar_file)?;

        let mut archive = Archive::new(tarball);

        let mut found_files = Vec::new();
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?.to_path_buf();
            found_files.push(path);
        }

        assert_eq!(found_files.len(), 1);
        assert_eq!(found_files[0], PathBuf::from(".hidden.txt"));

        Ok(())
    }

    #[test]
    fn should_exclude_git_dir() -> Result<()> {
        // should exclude .git/

        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;

        let git_dir = tempdir.path().join(".git");
        fs::create_dir(&git_dir)?;

        // write a file so the directory is non-empty
        let git_file = git_dir.join("test.txt");
        fs::write(git_file, "test")?;

        // write a root file so the tarball is non-empty
        let test_file = tempdir.path().join("test.txt");
        fs::write(test_file, "test")?;

        let tarball = create(tempdir.path(), tar_file)?;

        let mut archive = Archive::new(tarball);

        let mut found_files = Vec::new();
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?.to_path_buf();
            found_files.push(path);
        }

        // should only contain /test.txt
        assert_eq!(found_files.len(), 1);
        assert_eq!(found_files[0], PathBuf::from("test.txt"));

        Ok(())
    }

    #[test]
    fn should_exclude_empty_dir() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;

        let empty_dir = tempdir.path().join("empty_dir");
        fs::create_dir(&empty_dir)?;

        // create a fill at the root so the tarball isn't empty
        let file_path = tempdir.path().join("test.txt");
        fs::write(&file_path, "test")?;

        let tarball = create(tempdir.path(), tar_file)?;

        let mut archive = Archive::new(tarball);

        let mut found_files = Vec::new();
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?.to_path_buf();
            found_files.push(path);
        }
        assert_eq!(found_files.len(), 1);
        assert_eq!(found_files[0], PathBuf::from("test.txt"));

        for file in &found_files {
            assert!(file.file_name().is_some());
        }

        Ok(())
    }

    #[test]
    fn should_fail_bad_permission() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;
        let filepath = tempdir.path().to_path_buf().join("file.txt");
        fs::write(&filepath, "test")?;
        let mut perms = fs::metadata(&filepath)?.permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&filepath, perms)?;
        let result = create(tempdir.path(), tar_file);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .starts_with("Failed to open file at path")
        );
        Ok(())
    }

    #[test]
    fn should_fail_nonexistent_root() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let nonexistent = PathBuf::from("/nonexistent/path");
        let result = create(&nonexistent, tar_file);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .starts_with("Failed to canonicalize path")
        );
        Ok(())
    }

    #[test]
    fn should_fail_not_dir_root() -> Result<()> {
        let tar_file = tempfile::tempfile()?;
        let tempdir = tempfile::tempdir()?;
        let filepath = tempdir.path().to_path_buf().join("file.txt");
        fs::write(&filepath, "test")?;
        let result = create(&filepath, tar_file);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .starts_with("Path is not a directory")
        );
        Ok(())
    }
}

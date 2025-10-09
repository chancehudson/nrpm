use std::env::temp_dir;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Component;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use nanoid::nanoid;
use tar::Archive;
use tar::EntryType;

use nargo_parse::*;

/// A structure that assumes it's the only reader/writer for a directory
#[derive(Clone, Debug)]
pub struct OnyxStorage {
    pub storage_path: PathBuf,
}

pub enum FileType {
    GitRefs,
    GitPack,
    Tarball,
}

impl Default for OnyxStorage {
    fn default() -> Self {
        let storage_path = temp_dir().join(nanoid!());
        fs::create_dir(&storage_path).unwrap();
        Self { storage_path }
    }
}

impl OnyxStorage {
    pub fn new(storage_path: PathBuf) -> Result<Self> {
        if !fs::exists(&storage_path)? {
            anyhow::bail!("Storage directory does not exist: {:?}", storage_path);
        }
        Ok(Self { storage_path })
    }

    fn name_to_path(&self, filename: &str) -> PathBuf {
        #[cfg(debug_assertions)]
        if filename.contains("/") {
            println!("WARNING: reader expects a filename, not a filepath");
        }
        self.storage_path.join(filename)
    }

    pub fn name_to_refs_path(&self, filename: &str) -> PathBuf {
        #[cfg(debug_assertions)]
        if filename.contains("/") {
            println!("WARNING: reader expects a filename, not a filepath");
        }
        self.storage_path.join(format!("git-refs-{filename}"))
    }

    pub fn name_to_pack_path(&self, filename: &str) -> PathBuf {
        #[cfg(debug_assertions)]
        if filename.contains("/") {
            println!("WARNING: reader expects a filename, not a filepath");
        }
        self.storage_path.join(format!("git-pack-{filename}"))
    }

    /// Get a reader for filename in this storage
    pub async fn reader_async(
        &self,
        filename: &str,
        file_type: FileType,
    ) -> Result<tokio::fs::File> {
        let read_path = match file_type {
            FileType::GitRefs => self.name_to_refs_path(filename),
            FileType::GitPack => self.name_to_pack_path(filename),
            FileType::Tarball => self.name_to_path(filename),
        };
        Ok(tokio::fs::File::open(read_path).await?)
    }

    /// Take a tarball and look through it to make sure it's safe-ish, and contains a valid
    /// Nargo.toml
    ///
    /// Extract metadata from the Nargo.toml and return it.
    ///
    /// Here we check that the contents of a tarball are of bounded size, and bounded number of
    /// entries. We check all path entries and disallow absolute paths, and paths referencing parent
    /// directories. We disallow all non-regular files. We disallow file paths that are non-utf8.
    /// We disallow file paths that are empty.
    pub fn validate_tarball(&self, file: &mut File) -> Result<(String, String)> {
        file.seek(SeekFrom::Start(0))?;
        let mut archive = Archive::new(file);

        // maximum allowable size for the contents of the tarball
        const MAX_ARCHIVE_SIZE: u64 = 20 * 1024 * 1024;
        const MAX_ARCHIVE_ENTRIES: u64 = 10_000;
        // total number of bytes in the tarball
        let mut total_size = 0u64;
        let mut total_entries = 0u64;

        let mut nargo_toml_bytes = None;
        for entry in archive.entries()? {
            let mut entry = entry?;
            total_entries += 1;
            if total_entries > MAX_ARCHIVE_ENTRIES {
                anyhow::bail!("archive contains too many entries: {} files", total_entries);
            }
            total_size = total_size.saturating_add(entry.size());
            if total_size > MAX_ARCHIVE_SIZE {
                anyhow::bail!("archive too large: {} bytes", total_size);
            }
            let path = entry.path()?.to_path_buf();
            if path.is_absolute() {
                anyhow::bail!("absolute paths are disallowed in tarballs!");
            }
            if path.as_os_str().len() == 0 {
                anyhow::bail!("tarball contains entry with empty name");
            }
            path.to_str()
                .with_context(|| "tarball entry path contains non-unicode characters")?;
            for component in path.components() {
                match component {
                    Component::Normal(_) => {}
                    _ => {
                        anyhow::bail!("only normal path components are allowed in tarball entries!")
                    }
                }
            }
            match entry.header().entry_type() {
                EntryType::Regular => {
                    // TODO: safety checks here
                    if path == PathBuf::from("Nargo.toml") {
                        let mut bytes = Vec::default();
                        entry.read_to_end(&mut bytes)?;
                        nargo_toml_bytes = Some(bytes);
                    }
                }
                EntryType::Directory => {
                    continue;
                }
                EntryType::Link | EntryType::Symlink => anyhow::bail!(
                    "Tar contains link or symlink. Only directories and files are allowed in package tarballs!"
                ),
                _ => anyhow::bail!(
                    "Irregular entry detected in tar archive. Only directories and files are allowed in package tarballs!"
                ),
            }
        }
        if nargo_toml_bytes.is_none() {
            anyhow::bail!("Nargo.toml does not exist in package root!");
        }
        let nargo_toml_bytes = nargo_toml_bytes.unwrap();
        let config = NargoConfig::from_str(&String::try_from(nargo_toml_bytes)?)?;
        config.validate_metadata()?;

        Ok((
            config.package.name,
            config.package.version.unwrap_or_default(),
        ))
    }

    /// Ingest a tarball by performing sanity/safety checks, extracting to directory, and creating
    /// a mocked git response for Nargo compatibility.
    pub fn ingest_tarball(
        &self,
        file: &mut File,
        filename: String,
        version_name: &str,
    ) -> Result<()> {
        #[cfg(debug_assertions)]
        if self.contains_filename(&filename, FileType::Tarball)? {
            panic!("inserting filename that already exists in OnyxStorage");
        }

        file.seek(SeekFrom::Start(0))?;
        let (refs_res, pack_res) = nrpm_tarball::extract_git_mock(file, version_name)?;
        let mut refs_file = File::create(self.name_to_refs_path(&filename))?;
        let mut pack_file = File::create(self.name_to_pack_path(&filename))?;
        refs_file.write_all(&refs_res)?;
        pack_file.write_all(&pack_res)?;

        let to_path = self.name_to_path(&filename);

        file.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![];
        file.read_to_end(&mut bytes)?;
        let mut to_file = File::create(to_path)?;
        to_file.write_all(&mut bytes)?;
        Ok(())
    }

    pub fn contains_filename(&self, filename: &str, file_type: FileType) -> Result<bool> {
        let path = match file_type {
            FileType::GitRefs => self.name_to_refs_path(filename),
            FileType::GitPack => self.name_to_pack_path(filename),
            FileType::Tarball => self.name_to_path(filename),
        };
        Ok(fs::exists(path)?)
    }
}

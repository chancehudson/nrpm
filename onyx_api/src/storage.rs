use std::env::temp_dir;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use nanoid::nanoid;

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

    /// Ingest a tarball by performing sanity/safety checks, extracting to directory, and creating
    /// a mocked git response for Nargo compatibility.
    pub fn ingest_tarball(&self, file: &mut File, filename: String) -> Result<()> {
        #[cfg(debug_assertions)]
        if self.contains_filename(&filename, FileType::Tarball)? {
            panic!("inserting filename that already exists in OnyxStorage");
        }

        file.seek(SeekFrom::Start(0))?;
        let (refs_res, pack_res) = nrpm_tarball::extract_git_mock(file)?;
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

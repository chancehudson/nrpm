use std::env::temp_dir;
use std::fs;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use anyhow::Result;
use nanoid::nanoid;

/// A structure that assumes it's the only reader/writer for a directory
#[derive(Clone, Debug)]
pub struct OnyxStorage {
    pub storage_path: PathBuf,
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
        Ok(Self { storage_path })
    }

    fn name_to_path(&self, filename: &str) -> PathBuf {
        #[cfg(debug_assertions)]
        if filename.contains("/") {
            println!("WARNING: reader expects a filename, not a filepath");
        }
        self.storage_path.join(filename)
    }

    /// Get a reader for filename in this storage
    pub async fn reader_async(&self, filename: &str) -> Result<tokio::fs::File> {
        let read_path = self.name_to_path(&filename);
        Ok(tokio::fs::File::open(read_path).await?)
    }

    /// Move a file at a path into this storage
    pub fn ingest_file(&self, file: &mut File, filename: String) -> Result<()> {
        #[cfg(debug_assertions)]
        if self.contains_filename(&filename)? {
            panic!("inserting filename that already exists in OnyxStorage");
        }
        let to_path = self.name_to_path(&filename);

        file.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![];
        file.read_to_end(&mut bytes)?;
        let mut to_file = File::create(to_path)?;
        to_file.write_all(&mut bytes)?;
        Ok(())
    }

    pub fn contains_filename(&self, filename: &str) -> Result<bool> {
        let path = self.name_to_path(filename);
        Ok(fs::exists(path)?)
    }
}

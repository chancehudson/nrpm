use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use toml::Table;

use nargo_parse::*;

#[derive(Clone, Debug)]
pub struct Lockfile {
    pub version: i64,
    packages_cache: HashMap<String, LockEntry>,
}

impl Lockfile {
    pub fn new() -> Self {
        Self {
            version: 0,
            packages_cache: HashMap::default(),
        }
    }

    /// Load from file, parse, and build a hashmap of entries.
    pub fn load_or_init(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let mut s: HashMap<String, toml::Value> = toml::from_str(&std::fs::read_to_string(path)?)?;
        let packages = match s.remove("packages").unwrap_or(toml::Value::Array(vec![])) {
            toml::Value::Array(packages) => packages
                .into_iter()
                .map(|v| {
                    v.try_into().map_err(|e| {
                        anyhow::anyhow!("failed to parse lockfile package entry {e:?}")
                    })
                })
                .collect::<Result<Vec<LockEntry>>>()?,
            _ => anyhow::bail!("malformed lockfile, packages must be an array"),
        };
        let mut packages_cache = HashMap::default();
        for entry in packages {
            let entry_identifier = entry.identifier();
            if packages_cache.contains_key(&entry_identifier) {
                println!(
                    "WARNING: lockfile contains a duplicate entry for {}:{}",
                    entry.git, entry.tag
                );
            }
            packages_cache.insert(entry_identifier, entry);
        }
        let version = match s.get("version").ok_or(anyhow::anyhow!(
            "malformed lockfile, does not contain version"
        ))? {
            toml::Value::Integer(version) => *version,
            _ => anyhow::bail!("malformed lockfile, version must be an integer"),
        };
        if version != 0 {
            anyhow::bail!(
                "bad version number, only version 0 is supported by this version of nrpm"
            );
        }
        Ok(Self {
            version,
            packages_cache,
        })
    }

    pub fn entries(&self) -> impl Iterator<Item = &LockEntry> {
        self.packages_cache.values()
    }

    /// Retrieve a lockfile entry, if it exists.
    pub fn entry(&self, identifier: &str) -> Option<LockEntry> {
        self.packages_cache
            .get(identifier)
            .and_then(|entry| Some(entry.clone()))
    }

    /// Serialize and write to file. This involves transforming the packages cache to a simple vec.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut out = HashMap::<String, toml::Value>::default();
        out.insert("version".into(), toml::Value::Integer(0));
        out.insert(
            "packages".into(),
            toml::Value::Array(
                self.packages_cache
                    .iter()
                    .map(|(_, val)| Table::try_from(val).map_err(|e| anyhow::anyhow!(e)))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .map(|v| toml::Value::Table(v))
                    .collect::<Vec<_>>(),
            ),
        );
        let str = toml::to_string_pretty(&out)?;
        std::fs::write(path, str)?;
        Ok(())
    }

    /// Insert a dependence that exists at `path` into the lockfile
    ///
    /// The contents at `path` will be hashed.
    pub fn upsert(&mut self, dep: Dependency, path: &Path) -> Result<()> {
        if !path.is_absolute() {
            anyhow::bail!("lockfile paths must be absolute");
        }
        let hash = nrpm_tarball::hash_dir(path)?;
        if let Some(git) = &dep.git
            && let Some(tag) = &dep.tag
        {
            self.packages_cache.insert(
                dep.identifier()?,
                LockEntry {
                    git: git.clone(),
                    tag: tag.clone(),
                    blake3: hash.to_string(),
                },
            );
        }

        Ok(())
    }

    pub fn remove(&mut self, identifier: &str) {
        self.packages_cache.remove(identifier);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockEntry {
    pub git: String,
    pub tag: String,
    pub blake3: String, // Content hash of the package
}

impl LockEntry {
    pub fn identifier(&self) -> String {
        format!("{}@{}", self.git, self.tag)
    }
}

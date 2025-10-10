use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use anyhow::Result;
use reqwest::Url;
use serde::Deserialize;
use serde::Serialize;

/// Represents the contents of a `Nargo.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NargoConfig {
    pub package: Package,
    #[serde(default)]
    dependencies: toml::Table,
}

impl NargoConfig {
    pub fn from_str(str: &str) -> Result<Self> {
        Ok(toml::from_str::<Self>(str)?)
    }

    /// Load a Nargo.toml and parse it into into a `NargoConfig`
    ///
    /// `path` may be either a `Nargo.toml` file, or a directory containing a `Nargo.toml` file.
    pub fn load(path: &Path) -> Result<Self> {
        let nargo_path = if path.is_dir() {
            path.join("Nargo.toml")
        } else {
            path.to_path_buf()
        };
        if let Err(e) = std::fs::metadata(&nargo_path) {
            log::debug!("{e:?}");
            anyhow::bail!("Unable to stat path: {:?}", nargo_path);
        }
        let mut str = String::default();
        File::open(nargo_path)?.read_to_string(&mut str)?;
        Self::from_str(&str)
    }

    pub fn add_dependencies_in_place(path: &Path, new_dependencies: Vec<Dependency>) -> Result<()> {
        let nargo_path = if path.is_dir() {
            path.join("Nargo.toml")
        } else {
            path.to_path_buf()
        };
        let mut str = String::default();
        File::open(&nargo_path)?.read_to_string(&mut str)?;
        let mut doc = str.parse::<toml_edit::DocumentMut>()?;
        if doc.get("dependencies").is_none() {
            doc.insert(
                "dependencies",
                toml_edit::Item::Table(toml_edit::Table::new()),
            );
        }
        let dependencies = doc
            .get_mut("dependencies")
            .expect("dependencies should exist");
        for dep in new_dependencies {
            if dependencies.get(&dep.name).is_some() {
                anyhow::bail!(
                    "package \"{}\" already exists in Nargo.toml dependencies\nRemove the existing entry to install",
                    dep.name
                );
            }
            let mut table = toml_edit::InlineTable::new();
            for (key, val) in dep.to_value() {
                table.insert(&key, val.into());
            }
            dependencies
                .as_table_mut()
                .ok_or(anyhow::anyhow!("dependencies is not a table in Nargo.toml"))?
                .insert(&dep.name, table.into());
        }
        std::fs::write(&nargo_path, doc.to_string())?;

        Ok(())
    }

    /// Validates package metadata. Currently does semver validation for version field.
    pub fn validate_metadata(&self) -> Result<()> {
        semver::Version::parse(self.package.version.as_ref().ok_or(anyhow::anyhow!(
            "version field is not present in package section"
        ))?)
        .with_context(|| "Failed to parse version as semver")?;
        Ok(())
    }

    /// Check that all the dependencies in this `Nargo.toml` are configured correctly.
    pub fn validate_dependencies(&self) -> Result<()> {
        for (name, dep) in self.dependencies()? {
            dep.valid_or_err().map_err(|e| {
                anyhow::anyhow!(
                    "in package {} dependency {} is misconfigured: {:?}",
                    self.package.name,
                    name,
                    e
                )
            })?;
        }
        Ok(())
    }

    /// TODO: cache this. Potentially lots of extra parsing here.
    pub fn dependencies(&self) -> Result<HashMap<String, Dependency>> {
        let mut dependencies = HashMap::new();
        for (name, val) in &self.dependencies {
            if let Ok(mut dep) = val.clone().try_into::<Dependency>() {
                dep.name = name.clone();
                dependencies.insert(name.clone(), dep);
            } else {
                anyhow::bail!(
                    "failed to parse dependency {} in package {}",
                    name,
                    self.package.name
                );
            }
        }
        Ok(dependencies)
    }
}

/// Represents the `package` section of a `Nargo.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub authors: Option<Vec<String>>,
    pub repository: Option<String>,
    pub keywords: Option<Vec<String>>,
}

/// Represents each entry in the `dependencies` section of a `Nargo.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    #[serde(skip)]
    pub name: String,
    pub git: Option<String>,
    pub tag: Option<String>, // Nargo resolves this as a git clone --branch argument: https://github.com/noir-lang/noir/blob/12e90c0d51fc53998a2b75d6fb302d621227accd/tooling/nargo_toml/src/git.rs#L51
    pub directory: Option<String>, // Allows a module to reside inside a subdirectory of a package.
    pub path: Option<String>,
}

impl Dependency {
    pub fn new_git(name: String, url: String, tag: String) -> Self {
        Self {
            name,
            git: Some(url),
            tag: Some(tag),
            directory: None,
            path: None,
        }
    }

    pub fn to_value(&self) -> HashMap<String, String> {
        let mut content = HashMap::new();
        if let Some(git) = &self.git {
            content.insert("git".to_string(), git.clone());
        }
        if let Some(tag) = &self.tag {
            content.insert("tag".to_string(), tag.clone());
        }
        if let Some(path) = &self.path {
            content.insert("path".to_string(), path.clone());
        }
        if let Some(directory) = &self.directory {
            content.insert("directory".to_string(), directory.clone());
        }
        content
    }

    pub fn is_local(&self) -> bool {
        self.path.is_some()
    }

    /// A distinct identifier for this dependence. Dependencies with equal identifiers
    /// should be pointing to the same content.
    pub fn identifier(&self) -> Result<String> {
        if let Some(git) = &self.git
            && let Some(tag) = &self.tag
        {
            Ok(format!("{}@{}", git, tag))
        } else if let Some(path) = &self.path {
            Ok(format!("{}", path))
        } else {
            anyhow::bail!("invalid dependency configuration");
        }
    }

    /// Validate the dependence configuration. Ensure a proper combination of fields are set, and
    /// that local dependencies exist.
    pub fn valid_or_err(&self) -> Result<()> {
        if self.path.is_some() && self.git.is_some() {
            anyhow::bail!("path and git may not both be specified for dependence");
        } else if self.path.is_some() && self.tag.is_some() {
            anyhow::bail!("path and tag may not both be specified for dependence");
        } else if self.git.is_some() && self.tag.is_none() {
            anyhow::bail!("git dependencies must specify a tag");
        }
        if let Some(dir_str) = &self.directory
            && PathBuf::from(dir_str).is_absolute()
        {
            anyhow::bail!("directory must be relative");
        }
        if let Some(path_str) = &self.path {
            let path = PathBuf::from_str(path_str)
                .map_err(|_| anyhow::anyhow!("failed to parse path: {}", path_str))?;
            let canonical = std::fs::canonicalize(&path).map_err(|e| {
                anyhow::anyhow!("failed to canonicalize path: {} {:?}", path_str, e)
            })?;
            match std::fs::metadata(&canonical) {
                Ok(metadata) => {
                    if !metadata.is_dir() {
                        anyhow::bail!("dependence path is pointing to a non-directory");
                    }
                }
                Err(e) => anyhow::bail!("unable to state dependence path: {} {:?}", path_str, e),
            }
        }
        Ok(())
    }

    /// Determine a distinct path for a folder in the shared system cache.
    ///
    /// https://github.com/noir-lang/noir/blob/12e90c0d51fc53998a2b75d6fb302d621227accd/tooling/nargo_toml/src/git.rs#L51
    pub fn folder_path(&self, system_cache_path: &Path) -> Result<PathBuf> {
        let mut folder = system_cache_path.to_path_buf();
        if let Some(git) = &self.git
            && let Some(tag) = &self.tag
        {
            let url = Url::parse(git)?;
            let domain = url
                .domain()
                .ok_or(anyhow::anyhow!("git url did not contain a domain: {}", git))?;
            folder.push(domain.trim_start_matches("/"));
            folder.push(url.path().trim_start_matches("/"));
            folder.push(tag.trim_start_matches("/"));
            Ok(folder)
        } else {
            anyhow::bail!("cannot determine folder name for non-git dependence")
        }
    }

    /// Compute the path of the module relative to the package root directory.
    pub fn module_path(&self, pkg_path: &Path) -> Result<PathBuf> {
        if let Some(dir) = &self.directory {
            let dir_path = PathBuf::from(dir);
            assert!(!dir_path.is_absolute(), "directory must not be absolute");
            Ok(pkg_path.join(dir_path))
        } else {
            Ok(pkg_path.to_path_buf())
        }
    }
}

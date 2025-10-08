use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::Result;
use indicatif::ProgressStyle;
use onyx_api::prelude::*;
use reqwest::Url;
use serde::Deserialize;

/// Represents the contents of a `Nargo.toml` file.
#[derive(Debug, Clone, Deserialize)]
pub struct NargoConfig {
    pub package: Package,
    #[serde(default)]
    dependencies: toml::Table,
}

impl NargoConfig {
    /// TODO: cache this. Potentially lots of extra parsing here.
    pub fn dependencies(&self) -> Result<HashMap<String, Dependency>> {
        let mut dependencies = HashMap::new();
        for (name, val) in &self.dependencies {
            if let Ok(dep) = val.clone().try_into::<Dependency>() {
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
}

/// Represents the `package` section of a `Nargo.toml` file.
#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: Option<String>,
}

/// Represents each entry in the `dependencies` section of a `Nargo.toml` file.
#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    pub git: Option<String>,
    pub tag: Option<String>, // Nargo resolves this as a git clone --branch argument: https://github.com/noir-lang/noir/blob/12e90c0d51fc53998a2b75d6fb302d621227accd/tooling/nargo_toml/src/git.rs#L51
    pub directory: Option<String>, // Allows a module to reside inside a subdirectory of a package.
    pub path: Option<String>,
}

impl Dependency {
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

/// A command to read a Nargo.toml file and retrieve all direct and indirect dependencies.
///
/// We have a few kinds of dependencies to resolve.
///
/// 1. Git URL. This requires cloning the repository at a specific tag.
/// 2. Package name. This will load the package from the nrpm registry.
/// 3. Local path. Read the contents of a directory on the local machine.
pub async fn install(api: &OnyxApi, path: PathBuf) -> Result<()> {
    // Match the nargo default path.
    // TODO: make this more configurable
    //
    // https://github.com/noir-lang/noir/blob/12e90c0d51fc53998a2b75d6fb302d621227accd/tooling/nargo_toml/src/git.rs#L51
    let dep_cache_path = dirs::home_dir()
        .expect("unable to determine user home directory")
        .join("nargo");
    if dep_cache_path.exists() && !dep_cache_path.is_dir() {
        anyhow::bail!(
            "Global dependency cache is a non-directory! {:?}",
            dep_cache_path
        );
    } else if !dep_cache_path.exists() {
        std::fs::create_dir(&dep_cache_path)?;
    }
    let progress = indicatif::ProgressBar::new_spinner();
    let multiprogress = indicatif::MultiProgress::new();
    let progress = multiprogress.add(progress);
    progress.enable_steady_tick(Duration::from_millis(50));
    progress.set_message("Initializing...");

    multiprogress.insert_before(
        &progress,
        indicatif::ProgressBar::new(0)
            .with_prefix("🎄 Building dep tree...")
            .with_style(ProgressStyle::with_template("{prefix}")?)
            .with_finish(indicatif::ProgressFinish::Abandon),
    );

    multiprogress.insert_before(
        &progress,
        indicatif::ProgressBar::new(0)
            .with_prefix("⬇️  Downloading dependencies...")
            .with_style(ProgressStyle::with_template("{prefix}")?)
            .with_finish(indicatif::ProgressFinish::Abandon),
    );

    // let's look for a Nargo.toml
    // identifiers keyed to config
    let mut all_dependencies = HashMap::<String, NargoConfig>::default();
    let mut pending_dependencies = vec![path];
    while let Some(pkg_path) = pending_dependencies.pop() {
        let config = load_nargo_config(&pkg_path)?;
        progress.set_message(format!("{}: resolving", config.package.name));
        // check that our configuration is sane/valid
        config.validate_dependencies()?;
        // for each direct dependency let's load if needed.
        // TODO: read the lockfile if it exists in each dependency
        for (name, dep) in config.dependencies()? {
            let identifier = dep.identifier()?;
            if all_dependencies.contains_key(&identifier) {
                // we've already loaded this dep and validated it, skip
                continue;
            }
            all_dependencies.insert(identifier, config.clone());
            if let Some(dep_path) = &dep.path {
                // dependency is a local path, nothing to load
                let dep_path = PathBuf::from(dep_path);
                pending_dependencies.push(if dep_path.is_absolute() {
                    dep.module_path(&dep_path)?
                } else {
                    dep.module_path(&pkg_path.join(dep_path))?
                });
                continue;
            }
            let dep_root_path = dep.folder_path(&dep_cache_path)?;
            pending_dependencies.push(dep.module_path(&dep_root_path)?);
            if std::fs::exists(&dep_root_path)? {
                // dependency is already in the system cache
                progress.set_message(format!("{}: exists in cache", name));
                continue;
            }
            progress.set_message(format!("{}: git clone", name));
            // otherwise we need to load the dependence
            let tag = dep.tag.expect("tag should be Some at this point");
            let git_url = dep.git.expect("git should be Some at this point");

            // download atomically
            // clone into a tmpdir then move it into place
            let workdir = tempfile::tempdir()?.keep();
            std::process::Command::new("git")
                .arg("-c")
                .arg("advice.detachedHead=false")
                .arg("clone")
                .arg("--depth")
                .arg("1")
                .arg("--branch")
                .arg(&tag)
                .arg(&git_url)
                .arg(
                    workdir
                        .to_str()
                        .expect("tempdir has non-unicode characters"),
                )
                .output()?;
            std::fs::create_dir_all(&dep_root_path)?;
            std::fs::rename(workdir, &dep_root_path)?;
        }
    }
    progress.finish_and_clear();
    // for each dep we need to load all it's deps
    // println!("{:?}", nargo_content);
    Ok(())
}

/// Load a Nargo.toml and parse it into into a `NargoConfig`
///
/// `path` may be either a `Nargo.toml` file, or a directory containing a `Nargo.toml` file.
fn load_nargo_config(path: &Path) -> Result<NargoConfig> {
    let nargo_path = if path.is_dir() {
        path.join("Nargo.toml")
    } else {
        path.to_path_buf()
    };
    if let Err(e) = std::fs::metadata(&nargo_path) {
        anyhow::bail!("Unable to stat path: {:?} {:?}", path, e);
    }
    let mut str = String::default();
    File::open(nargo_path)?.read_to_string(&mut str)?;
    Ok(toml::from_str::<NargoConfig>(&str)?)
}

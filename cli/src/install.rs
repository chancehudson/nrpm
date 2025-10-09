use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use indicatif::ProgressStyle;
use nargo_parse::*;
use onyx_api::prelude::*;

use crate::lockfile::Lockfile;

/// A command to read a Nargo.toml file and retrieve all direct and indirect dependencies.
///
/// We have a few kinds of dependencies to resolve.
///
/// 1. Git URL. This requires cloning the repository at a specific tag.
/// 2. Package name. This will load the package from the nrpm registry.
/// 3. Local path. Read the contents of a directory on the local machine.
pub async fn install(_api: &OnyxApi, path: PathBuf) -> Result<()> {
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
    // try to load the Nargo.toml in the target directory here
    // bail with a helpful error message if it's not there
    NargoConfig::load(&path)
        .with_context(|| "Unable to find a Nargo.toml in the target directory")?;

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
    let mut all_dependencies = HashMap::<String, (PathBuf, Dependency)>::default();
    let mut all_lockfiles = HashMap::<String, Lockfile>::default();
    let mut pending_dependencies = vec![path.clone()];
    while let Some(pkg_path) = pending_dependencies.pop() {
        let config = NargoConfig::load(&pkg_path)?;
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

            // dependency is a local path, nothing to load
            if let Some(dep_path_str) = &dep.path {
                let dep_path = PathBuf::from(dep_path_str);
                let module_path = if dep_path.is_absolute() {
                    dep.module_path(&dep_path)?
                } else {
                    dep.module_path(&pkg_path.join(&dep_path))?
                };
                all_dependencies.insert(identifier.clone(), (module_path.clone(), dep.clone()));
                all_lockfiles.insert(
                    identifier,
                    Lockfile::load_or_init(&dep_path.join("nrpm.lock"))?,
                );
                pending_dependencies.push(module_path);
                continue;
            }
            let dep_root_path = dep.folder_path(&dep_cache_path)?;
            pending_dependencies.push(dep.module_path(&dep_root_path)?);
            if std::fs::exists(&dep_root_path)? {
                // dependency is already in the system cache
                progress.set_message(format!("{}: exists in cache", name));
                all_lockfiles.insert(
                    identifier.clone(),
                    Lockfile::load_or_init(&dep_root_path.join("nrpm.lock"))?,
                );
                all_dependencies.insert(
                    identifier.clone(),
                    (dep.module_path(&dep_root_path)?, dep.clone()),
                );
                continue;
            }
            progress.set_message(format!("{}: git clone", name));
            // otherwise we need to load the dependence
            let tag = dep.tag.as_ref().expect("tag should be Some at this point");
            let git_url = dep.git.as_ref().expect("git should be Some at this point");

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
                .arg(tag)
                .arg(git_url)
                .arg(
                    workdir
                        .to_str()
                        .expect("tempdir has non-unicode characters"),
                )
                .output()?;
            std::fs::create_dir_all(&dep_root_path)?;
            std::fs::rename(workdir, &dep_root_path)?;
            all_lockfiles.insert(
                identifier.clone(),
                Lockfile::load_or_init(&dep_root_path.join("nrpm.lock"))?,
            );
            all_dependencies.insert(
                identifier.clone(),
                (dep.module_path(&dep_root_path)?, dep.clone()),
            );
        }
    }
    multiprogress.insert_before(
        &progress,
        indicatif::ProgressBar::new(0)
            .with_prefix("🧮 Checking integrity...")
            .with_style(ProgressStyle::with_template("{prefix}")?)
            .with_finish(indicatif::ProgressFinish::Abandon),
    );
    progress.set_message("computing hashes");
    let lockfile_path = path.join("nrpm.lock");
    let mut hashes = HashMap::<String, String>::default();
    for (dep_path, dep) in all_dependencies.values() {
        hashes.insert(
            dep.identifier()?,
            nrpm_tarball::hash_dir(dep_path)?.to_string(),
        );
    }
    progress.set_message("checking dependent lockfiles");
    // check the lockfiles of all our dependencies
    for (_identifier, lockfile) in all_lockfiles {
        for entry in lockfile.entries() {
            let entry_identifier = entry.identifier();
            let hash = hashes.get(&entry_identifier).ok_or(anyhow::anyhow!(
                "unknown lockfile identifier {}",
                entry_identifier
            ))?;
            if hash != &entry.blake3 {
                let (_, dep) = all_dependencies
                    .get(&entry_identifier)
                    .ok_or(anyhow::anyhow!(
                        "dependency was not enumerated {}",
                        entry.git
                    ))?;
                anyhow::bail!("hash mismatch for package {}!", dep.name);
            }
        }
    }
    progress.set_message("checking lockfile");
    // now check our lockfile
    let mut lockfile = Lockfile::load_or_init(&lockfile_path)?;
    // first remove any dependencies that no longer exist in the tree
    for entry in lockfile.entries().cloned().collect::<Vec<_>>() {
        let entry_identifier = entry.identifier();
        if !all_dependencies.contains_key(&entry_identifier) {
            lockfile.remove(&entry_identifier);
        }
    }
    // then add and verify all dependencies
    for (dep_path, dep) in all_dependencies.values() {
        if dep.is_local() {
            continue;
        }
        if let Some(entry) = lockfile.entry(&dep.identifier()?) {
            let entry_identifier = entry.identifier();
            // check that our existing hash matches
            let hash = hashes.get(&entry_identifier).ok_or(anyhow::anyhow!(
                "unknown lockfile identifier {}",
                entry_identifier
            ))?;
            if hash != &entry.blake3 {
                anyhow::bail!("hash mismatch for dependent {}", dep.name);
            }
        } else {
            // add an entry
            lockfile.upsert(dep.clone(), dep_path)?;
        }
    }
    lockfile.save(&lockfile_path)?;
    multiprogress.insert_before(
        &progress,
        indicatif::ProgressBar::new(0)
            .with_prefix("🟩 Done!")
            .with_style(ProgressStyle::with_template("{prefix}")?)
            .with_finish(indicatif::ProgressFinish::Abandon),
    );
    progress.finish_and_clear();
    Ok(())
}

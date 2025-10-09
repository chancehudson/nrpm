use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use nargo_parse::*;

use crate::lockfile::Lockfile;

/// A command to read a Nargo.toml file and retrieve all direct and indirect dependencies.
///
/// We have a few kinds of dependencies to resolve.
///
/// 1. Git URL. This requires cloning the repository at a specific tag.
/// 2. Package name. This will load the package from the nrpm registry.
/// 3. Local path. Read the contents of a directory on the local machine.
pub async fn install(path: PathBuf) -> Result<()> {
    // try to load the Nargo.toml in the target directory here
    // bail with a helpful error message if it's not there
    let root_pkg = NargoConfig::load(&path)
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
            .with_prefix("🌨️  Downloading dependencies...")
            .with_style(ProgressStyle::with_template("{prefix}")?)
            .with_finish(indicatif::ProgressFinish::Abandon),
    );

    let all_dependencies = download_dependencies(&root_pkg, &path, &progress)?;

    multiprogress.insert_before(
        &progress,
        indicatif::ProgressBar::new(0)
            .with_prefix("✨ Checking integrity...")
            .with_style(ProgressStyle::with_template("{prefix}")?)
            .with_finish(indicatif::ProgressFinish::Abandon),
    );

    progress.set_message("computing hashes");
    let lockfile_path = path.join("nrpm.lock");
    let mut hashes = HashMap::<String, String>::default();
    for (dep_path, dep, _config) in all_dependencies.values() {
        hashes.insert(
            dep.identifier()?,
            nrpm_tarball::hash_dir(dep_path)?.to_string(),
        );
    }

    progress.set_message("checking dependent lockfiles");
    let mut validated_lockfile_count = 0u64;
    for (dep_path, dep, config) in all_dependencies.values() {
        let dep_module_path = dep.module_path(dep_path)?;
        let lockfile = Lockfile::load_or_init(&dep_module_path.join("nrpm.lock"))?;

        if lockfile.is_empty() && config.dependencies()?.is_empty() {
            // if a package has no lockfile, but also has no dependencies we consider it validated
            validated_lockfile_count += 1;
            continue;
        } else if !lockfile.is_empty() {
            validated_lockfile_count += 1;
        }

        for entry in lockfile.entries() {
            let entry_identifier = entry.identifier();
            let hash = hashes.get(&entry_identifier).ok_or(anyhow::anyhow!(
                "unknown lockfile identifier {}",
                entry_identifier
            ))?;
            if hash != &entry.blake3 {
                // the dependency of the dependency we're checking
                let (inner_dep_path, inner_dep, _config) = all_dependencies
                    .get(&entry_identifier)
                    .ok_or(anyhow::anyhow!(
                        "dependency was not enumerated {}",
                        entry.git
                    ))?;
                Err(anyhow::anyhow!("ADVICE Consider deleting local copies and re-downloading. If this error persists contact the authors of \"{}\" and \"{}\".", dep.name, inner_dep.name)
                    .context("integrity check failed, halting")
                    .context(format!("\"{}\" exists at path: {dep_path:?}", dep.name))
                    .context(format!(
                        "\"{}\" exists at path: {inner_dep_path:?}",
                        inner_dep.name
                    ))
                    .context(format!(
                        "our local \"{}\" has hash: {}",
                        inner_dep.name, hash
                    ))
                    .context(format!(
                        "\"{}\" depends on \"{}\" with hash: {}",
                        dep.name, inner_dep.name, entry.blake3
                    ))
                    .context(format!(
                        "lockfile integrity check failed for dependency: \"{}\"",
                        dep.name
                    )))?;
            }
        }
    }
    progress.set_message("checking lockfile");
    // now check our lockfile
    let mut lockfile = Lockfile::load_or_init(&lockfile_path)?;
    validated_lockfile_count += 1;
    // first remove any dependencies that no longer exist in the tree
    // or that are local path references
    for entry in lockfile.entries().cloned().collect::<Vec<_>>() {
        let entry_identifier = entry.identifier();
        if let Some((_, dep, _)) = all_dependencies.get(&entry_identifier)
            && dep.is_local()
        {
            lockfile.remove(&entry_identifier);
        } else {
            lockfile.remove(&entry_identifier);
        }
    }
    // then add and verify all dependencies
    for (dep_path, dep, _config) in all_dependencies.values() {
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
                Err(anyhow::anyhow!("ADVICE Consider deleting local copy and re-downloading. If this error persists contact the author of \"{}\".", dep.name)
                    .context("integrity check failed, halting")
                    .context(format!("computed hash: {}", hash))
                    .context(format!("expected hash: {}", entry.blake3))
                    .context(format!("dependent location: {:?}", dep_path))
                    .context(format!(
                        "hash mismatch for dependent package: \"{}\"\n",
                        dep.name
                    )))?;
            }
        } else {
            // add an entry
            lockfile.upsert(dep.clone(), dep_path)?;
        }
    }
    lockfile.save(&lockfile_path)?;
    // all our dependencies, plus the root package
    let total_packages = all_dependencies.len() + 1;
    multiprogress.insert_before(
        &progress,
        indicatif::ProgressBar::new(0)
            .with_prefix(format!(
                "👻 {} package{}, {} validated\n✅ wrote {}",
                total_packages,
                if total_packages == 1 { "" } else { "s" },
                validated_lockfile_count,
                pathdiff::diff_paths(&lockfile_path, std::env::current_dir()?)
                    .unwrap_or(lockfile_path)
                    .display()
            ))
            .with_style(ProgressStyle::with_template("{prefix}")?)
            .with_finish(indicatif::ProgressFinish::Abandon),
    );
    progress.finish_and_clear();
    Ok(())
}

// Given an entry Nargo.toml resolve all dependencies to locations on disk.
fn download_dependencies(
    root_pkg: &NargoConfig,
    path: &Path,
    progress: &ProgressBar,
) -> Result<HashMap<String, (PathBuf, Dependency, NargoConfig)>> {
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

    // all direct and indirect dependencies for root_pkg
    // identifier keyed to package path (not module path), dependency structure, and Nargo config
    let mut all_dependencies = HashMap::<String, (PathBuf, Dependency, NargoConfig)>::default();

    let mut pending_resolution = vec![(path.to_path_buf(), root_pkg.clone())];
    while let Some((pkg_path, config)) = pending_resolution.pop() {
        progress.set_message(format!("{}: resolving", config.package.name));
        // check that our configuration is sane/valid
        config.validate_dependencies()?;
        // for each direct dependency let's load if needed.
        for (_name, dep) in config.dependencies()? {
            let identifier = dep.identifier()?;
            if all_dependencies.contains_key(&identifier) {
                // we've already loaded this dep and validated it, skip
                continue;
            }

            // dependency is a local path, nothing to load
            if let Some(dep_path_str) = &dep.path {
                let dep_path = PathBuf::from(dep_path_str);
                let dep_pkg_path = if dep_path.is_absolute() {
                    dep_path
                } else {
                    pkg_path.join(&dep_path)
                };
                let dep_module_path = dep.module_path(&dep_pkg_path)?;
                let dep_config = NargoConfig::load(&dep_module_path)
                    .context(format!("located at path: {:?}", dep_module_path))
                    .context(format!(
                        "failed to load Nargo.toml for dependency \"{}\"",
                        dep.name
                    ))?;
                all_dependencies.insert(
                    identifier.clone(),
                    (dep_pkg_path, dep.clone(), dep_config.clone()),
                );
                pending_resolution.push((dep_module_path, dep_config));
                continue;
            }
            let dep_root_path = dep.folder_path(&dep_cache_path)?;
            if std::fs::exists(&dep_root_path)? {
                // dependency is already in the system cache
                progress.set_message(format!("{}: exists in cache", dep.name));
                let module_path = dep.module_path(&dep_root_path)?;
                let config = NargoConfig::load(&module_path)
                    .context(format!("located at: {:?}", module_path))
                    .context(format!(
                        "failed to load Nargo.toml for dependency \"{}\"",
                        dep.name
                    ))?;
                all_dependencies.insert(
                    identifier.clone(),
                    (dep_root_path.clone(), dep.clone(), config.clone()),
                );
                pending_resolution.push((module_path, config));
                continue;
            }
            progress.set_message(format!("{}: git clone", dep.name));
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
            let module_path = dep.module_path(&dep_root_path)?;
            let config = NargoConfig::load(&module_path)
                .context(format!("located at: {:?}", module_path))
                .context(format!(
                    "Downloaded dependency \"{}\" does not contain a Nargo.toml",
                    dep.name
                ))?;
            all_dependencies.insert(
                identifier.clone(),
                (dep_root_path, dep.clone(), config.clone()),
            );
            pending_resolution.push((module_path, config));
        }
    }

    Ok(all_dependencies)
}

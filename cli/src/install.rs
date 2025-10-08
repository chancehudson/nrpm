use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use onyx_api::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct NargoConfig {
    package: Package,
    dependencies: toml::Table,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    git: String,
    tag: String,
    directory: Option<String>,
}

pub async fn install(api: &OnyxApi, path: PathBuf) -> Result<()> {
    println!("🎄 Building dep tree...");
    if let Ok(metadata) = std::fs::metadata(&path) {
        if !metadata.is_dir() {
            anyhow::bail!("Path is not a directory: {:?}", path);
        }
    } else {
        anyhow::bail!("Unable to stat path: {:?}", path);
    }
    // let's look for a Nargo.toml
    let nargo_path = path.join("Nargo.toml");
    if let Ok(metadata) = std::fs::metadata(&nargo_path) {
        if !metadata.is_file() {
            anyhow::bail!("Nargo.toml not found: {:?}", nargo_path);
        }
    } else {
        anyhow::bail!("Unable to stat Nargo.toml: {:?}", nargo_path);
    }
    let mut str = String::default();
    File::open(nargo_path)?.read_to_string(&mut str)?;
    let nargo_content = toml::from_str::<NargoConfig>(&str)?;
    // for each dep we need to load all it's deps
    println!("{:?}", nargo_content);
    Ok(())
}

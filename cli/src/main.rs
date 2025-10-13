use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use clap::Arg;
use clap::ArgAction;
use clap::Command;
use nanoid::nanoid;
use nargo_parse::Dependency;
use nargo_parse::NargoConfig;
use onyx_api::prelude::*;
use tokio;
use tokio::task::JoinSet;

mod install;
mod lockfile;
mod publish;

#[cfg(debug_assertions)]
const REGISTRY_URL: &str = "http://localhost:8080";
#[cfg(not(debug_assertions))]
const REGISTRY_URL: &str = "https://nrpm.io";

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    log::debug!("registry url: {REGISTRY_URL}");

    if let Err(err) = run().await {
        eprintln!("❌ {}", err);

        // Print all errors in the chain
        for (_i, cause) in err.chain().enumerate().skip(1) {
            if cause.to_string().starts_with("ADVICE") {
                eprintln!(
                    "💡 {}",
                    cause.to_string().trim_start_matches("ADVICE").trim()
                );
            } else {
                eprintln!("   {}", cause);
            }
        }

        std::process::exit(1);
    } else {
        Ok(())
    }
}

async fn run() -> Result<()> {
    let matches = cli().get_matches();
    let api = OnyxApi::default();
    let cwd = std::env::current_dir()?;
    if let Some(matches) = matches.subcommand_matches("publish") {
        let path = matches
            .get_one::<String>("path")
            .map(|p| {
                let in_path = PathBuf::from(p);
                if in_path.is_relative() {
                    cwd.join(in_path)
                } else {
                    in_path
                }
            })
            .unwrap_or(cwd);
        let archive_path = matches
            .get_one::<String>("archive")
            .and_then(|s| Some(PathBuf::from(s)));
        install::install(path.to_path_buf()).await?;
        publish::upload_tarball(&api, &path, archive_path).await?;
    } else if let Some(matches) = matches.subcommand_matches("install") {
        let path = matches
            .get_one::<String>("path")
            .map(|p| {
                let in_path = PathBuf::from(p);
                if in_path.is_relative() {
                    cwd.join(in_path)
                } else {
                    in_path
                }
            })
            .unwrap_or(cwd);

        // the user wants to install a package and add it to Nargo.toml, let's give it a shot
        let mut join_set: JoinSet<Result<Dependency>> = JoinSet::new();
        let packages_to_install = 
            matches.get_many::<String>("package_name").unwrap_or_default();
        for new_dep_name in packages_to_install{
            let new_dep_name = new_dep_name.clone();
            let api = api.clone();
            join_set.spawn(async move {
            let (package, version) = api.load_package_latest_version(&new_dep_name).await.context(format!("Unable to install package \"{new_dep_name}\""))?;
            println!("Adding package: {}@{}", package.name, version.name);
            let git_url = format!("{REGISTRY_URL}/{new_dep_name}");
            let tag = version.name;
            Ok(Dependency::new_git(new_dep_name.to_string(), git_url, tag))
            });
        }
        let mut new_packages: Vec<Dependency> = Vec::default();
        while let Some(dep) = join_set.join_next().await {
            let dep = dep??;
            new_packages.push(dep);
        }
        if !new_packages.is_empty(){
            NargoConfig::add_dependencies_in_place(&path, new_packages).context("Failed to write new dependencies to Nargo.toml")?;
        }
        install::install(path).await?;
    }
    Ok(())
}

async fn attempt_auth() -> Result<LoginResponse> {
    let proposed_token = nanoid!();
    // we'll create a token and open the web browser
    let url = format!("{REGISTRY_URL}/_/propose_token?token={proposed_token}");
    println!("    {url}");
    open::that(url)?;

    let api = OnyxApi::default();
    const MAX_ATTEMPTS: usize = 60;
    let mut attempts = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        match api.auth(proposed_token.clone()).await {
            Ok(login) => return Ok(login),
            Err(_) => {
                attempts += 1;
                if attempts >= MAX_ATTEMPTS {
                    anyhow::bail!("Timed out waiting for token to activate!")
                }
            }
        }
    }
}

fn cli() -> Command {
    Command::new("nrpm")
        .version("0.0.0")
        .about("Noir package manager")
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::Count)
                .help("Sets the level of verbosity"),
        )
        .subcommand(
            Command::new("publish")
                .about("publish a package to the registry")
                .arg(
                    Arg::new("archive")
                        .short('a')
                        .long("archive")
                        .value_name("path")
                        .action(ArgAction::Set).help("Generate a package tarball and save it to local file instead of uploading to registry"),
                ).arg(Arg::new("path").short('p').long("path").value_name("path").action(ArgAction::Set).help("Publish a package from a custom path"))
        )
        .subcommand(
            Command::new("install")
            .alias("i")
                .about("install dependencies for a local project")
                .arg(Arg::new("path").short('p').long("path").value_name("path").action(ArgAction::Set).help("Install dependencies for a package at a path"))
                .arg(Arg::new("package_name").value_name("package_name").action(ArgAction::Append))
                // .arg(clap::arg!([package_name] "Name of a package to install"))
                
        )
}

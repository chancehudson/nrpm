use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::Arg;
use clap::ArgAction;
use clap::Command;
use nanoid::nanoid;
use onyx_api::prelude::*;
use tempfile::tempfile;
use tokio;

mod publish;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = cli().get_matches();
    let api = OnyxApi::default();
    // let login = LoginResponse { user: (), token: (), expires_at: () }
    if let Some(matches) = matches.subcommand_matches("publish") {
        let cwd = std::env::current_dir()?;
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
        println!("📦 Packaging {:?}", path);
        let mut tarball = tarball_nrpm::create(path, tempfile()?)?;
        if let Some(archive_path) = matches.get_one::<String>("archive") {
            io::copy(&mut tarball, &mut File::create(archive_path)?)?;
        } else {
            println!("🔃 Redirecting to authorize");
            tokio::time::sleep(Duration::from_millis(500)).await;
            let login = attempt_auth().await?;
            publish::upload_tarball(login, &api, &mut tarball).await?;
        }
    }
    Ok(())
}

async fn attempt_auth() -> Result<LoginResponse> {
    let proposed_token = nanoid!();
    // we'll create a token and open the web browser
    #[cfg(debug_assertions)]
    let url = "http://localhost:8080";
    #[cfg(not(debug_assertions))]
    let url = "https://nrpm.io";
    let url = format!("{url}/propose_token?token={proposed_token}");
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
}

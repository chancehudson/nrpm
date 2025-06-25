use std::fs::File;
use std::io;
use std::io::Seek;
use std::path::PathBuf;

use clap::Arg;
use clap::ArgAction;
use clap::Command;
use tokio;

mod publish;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = cli().get_matches();
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
        let tarball = publish::create_tarball(path)?;
        if let Some(archive_path) = matches.get_one::<String>("archive") {
            let mut tarball = tarball;
            // reset the file handle for copying to final destination
            tarball.seek(std::io::SeekFrom::Start(0))?;
            io::copy(&mut tarball, &mut File::create(archive_path)?)?;
            return Ok(());
        }
        publish::upload_tarball("http://127.0.0.1:3000/publish", tarball).await?;
    }
    Ok(())
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
                        .action(ArgAction::Set).help("Generate a package tarball and save it to local file instead of publishing"),
                ).arg(Arg::new("path").short('p').long("path").value_name("path").action(ArgAction::Set).help("Publish a package from a custom path"))
        )
}

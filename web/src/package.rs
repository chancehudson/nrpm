use std::{collections::HashMap, path::PathBuf};

use dioxus::prelude::*;
use onyx_api::prelude::*;

use nargo_parse::*;

use super::components::Header;

#[component]
pub fn PackageView(package_name: String) -> Element {
    let mut is_loading = use_signal(|| false);
    let mut status = use_signal(|| String::new());
    let mut package: Signal<Option<(PackageModel, PackageVersionModel)>> = use_signal(|| None);
    let mut package_config: Signal<Option<(NargoConfig, HashMap<PathBuf, Vec<u8>>)>> =
        use_signal(|| None);
    let mut package_hash_verified = use_signal(|| false);

    // On mount fetch the package metadata, load the package tarball, decompress and analyze
    use_effect(move || {
        let package_name = package_name.clone();
        spawn(async move {
            is_loading.set(true);

            // load the latest package version
            let api = OnyxApi::default();
            let (package, version) = match api.load_package_latest_version(&package_name).await {
                Ok(p) => {
                    package.set(Some(p.clone()));
                    p
                }
                Err(e) => {
                    status.set(format!("Error: {}", e));
                    is_loading.set(false);
                    return;
                }
            };

            // download the package tarball and extract to get the metadata
            let bytes = match api.download_tarball(&version.id).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    status.set(format!("Error: failed to download tarball bytes! {}", e));
                    is_loading.set(false);
                    return;
                }
            };
            let (_config, entries) = match nrpm_tarball::extract_metadata(bytes) {
                Ok(data) => {
                    package_config.set(Some(data.clone()));
                    data
                }
                Err(e) => {
                    status.set(format!("Error: failed to parse tarball bytes! {}", e));
                    is_loading.set(false);
                    return;
                }
            };

            match nrpm_tarball::hash_content(
                entries
                    .into_iter()
                    .map(|(path, data)| Ok(Some((path, data)))),
            ) {
                Ok(hash) => {
                    package_hash_verified.set(hash.to_string() == version.id.to_string());
                }
                Err(e) => {
                    status.set(format!("Error: failed to hash tarball content! {}", e));
                    is_loading.set(false);
                    return;
                }
            }
            is_loading.set(false);
        });
    });
    let package_inner = package.read();
    let package_config_inner = package_config.read();
    if package_inner.is_none() || package_config_inner.is_none() {
        return rsx! {
            Header { show_auth: true },
            h3 {
                "Loading..."
            }
            if !status.read().is_empty() {
                div {
                    style: "padding: 10px; border-radius: 4px; text-align: center; font-weight: bold;",
                    style: if status.read().contains("successful") {
                        "background-color: #d4edda; color: #155724; border: 1px solid #c3e6cb;"
                    } else {
                        "background-color: #f8d7da; color: #721c24; border: 1px solid #f5c6cb;"
                    },
                    "{status.read()}"
                }
            }
        };
    }
    let (package, version) = package_inner.as_ref().unwrap();
    let (package_config, package_contents) = package_config_inner.as_ref().unwrap();
    let readme_raw = package_contents
        .get(&PathBuf::from("README.md"))
        .map(|v| {
            String::from_utf8(v.clone()).unwrap_or("Error: README.md is not valid UTF8!".into())
        })
        .unwrap_or("No README.md found for this package!\n\nIf you're the author you should consider adding one 😊".into());

    let readme_html = ammonia::clean(&markdown::to_html(&readme_raw));

    rsx! {
        Header { show_auth: true },
        div {
            style: "padding: 40px; font-family: Arial, sans-serif;",

            div {
                style: "display: flex;
                       flex-direction: row;
                       flex-wrap: wrap;
                       justify-content: space-between;
                       align-items: flex-start;
                       margin-bottom: 4px;
                       padding-bottom: 4px;
                       ",
                div {
                    style: "display: flex;
                           flex-direction: column;
                           align-items: flex-start;
                           ",
                    h3 {
                        style: "margin: 0px; margin-bottom: 8px;",
                        "{package.name}@{version.name}"
                    }
                    for (path, data) in package_contents {
                        div {
                            style: "padding-left: 8px",
                            "{path.to_string_lossy()}"," - ","{data.len()}"," bytes"
                        }
                    }
                }
                div {
                    style: "display: flex;
                           flex-direction: column;
                           align-items: flex-start;
                           max-width: 300px;
                           ",
                    div {
                        "published {time_ago(version.created_at)}"
                    }
                    div {
                        "blake3: {version.id.to_string().chars().take(13).collect::<String>()}..."
                    },
                    if *package_hash_verified.read() {
                        div {
                            "✅ hash verified"
                        }
                    } else {
                        div {
                            "❌ hash mismatch!"
                        }
                    }
                    div {
                        style: "width: 100%; margin: 4px 0px; border-bottom: 1px solid black;"
                    },
                    div {
                        h4 {
                            style: "margin: 0px",
                            "Install"
                        }
                    },
                    div {
                        style: "padding: 8px; font-family: monospace; border: 1px solid gray; border-radius: 2px;",
                        "nrpm install {package.name}"
                    }
                    div {
                        style: "width: 100%; margin: 4px 0px; border-bottom: 1px solid black;"
                    },
                    if let Some(authors) = &package_config.package.authors && !authors.is_empty(){
                        div {
                            h4 {
                                style: "margin: 0px",
                                "Authors"
                            }
                        },
                        div {
                            style: "display: flex; flex-direction: row; flex-wrap: wrap; margin-left: 8px;",
                            for (i, author) in authors.iter().enumerate() {
                                div {
                                    style: "display: flex; flex-direction: row; flex-wrap: nowrap;",
                                    key: author,
                                    div {
                                        "{author}"
                                    },
                                    if i < authors.len() - 1 {
                                        div {
                                            key: author,
                                            style: "margin: 0px 8px",
                                            "|"
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            style: "width: 100%; margin: 4px 0px; border-bottom: 1px solid black;"
                        },
                    }
                    if let Some(description) = package_config.package.description.as_ref() {
                        div {
                            h4 {
                                style: "margin: 0px",
                                "Description"
                            }
                        }
                        div {
                            style: "margin-left: 8px; color: dimgray;",
                            "{description}"
                        }
                        div {
                            style: "width: 100%; margin: 4px 0px; border-bottom: 1px solid black;"
                        },
                    }
                    if let Some(repository) = package_config.package.repository.as_ref() {
                        div {
                            h4 {
                                style: "margin: 0px",
                                "Repository"
                            }
                        }
                        a {
                            style: "margin-left: 8px;",
                            href: "{repository}",
                            "{repository}"
                        }
                        div {
                            style: "width: 100%; margin: 4px 0px; border-bottom: 1px solid black;"
                        },
                    }
                    if let Some(keywords) = package_config.package.keywords.as_ref() {
                        div {
                            h4 {
                                style: "margin: 0px; margin-bottom: 4px;",
                                "Keywords"
                            }
                        }
                        div {
                            style: "margin-left: 8px; display: flex; flex-direction: row; flex-wrap: wrap;",
                            for keyword in keywords {
                                div {
                                    style: "margin-right: 8px; padding: 2px; border-radius: 4px; border: 1px solid black;",
                                    "{keyword}"
                                }
                            }
                        }
                        div {
                            style: "width: 100%; margin: 4px 0px; border-bottom: 1px solid black;"
                        },
                    }
                }
            }

            if !status.read().is_empty() {
                div {
                    style: "padding: 10px;
                           border-radius: 4px;
                           text-align: center;
                           font-weight: bold;
                           ",
                    style: if status.read().contains("successful") {
                        "background-color: #d4edda; color: #155724; border: 1px solid #c3e6cb;"
                    } else {
                        "background-color: #f8d7da; color: #721c24; border: 1px solid #f5c6cb;"
                    },
                    "{status.read()}"
                }
            }
            div {
                style: "background: #f5f5f5; padding: 4px; border-radius: 2px; border: 1px solid gray;",
                div {
                    dangerous_inner_html: readme_html
                }
            }
        }
    }
}

fn time_ago(timestamp: u64) -> String {
    let now = js_sys::Date::now() as u64 / 1000; // Current time in seconds
    let diff = now.saturating_sub(timestamp);

    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let minutes = diff / 60;
            format!(
                "{} minute{} ago",
                minutes,
                if minutes == 1 { "" } else { "s" }
            )
        }
        3600..=86399 => {
            let hours = diff / 3600;
            format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
        }
        86400..=604799 => {
            let days = diff / 86400;
            format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
        }
        604800..=2629743 => {
            let weeks = diff / 604800;
            format!("{} week{} ago", weeks, if weeks == 1 { "" } else { "s" })
        }
        2629744..=31556925 => {
            let months = diff / 2629744;
            format!("{} month{} ago", months, if months == 1 { "" } else { "s" })
        }
        _ => {
            let years = diff / 31556926;
            format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
        }
    }
}

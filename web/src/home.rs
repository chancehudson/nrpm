use dioxus::prelude::*;
use onyx_api::prelude::*;

use super::components::Header;

#[component]
pub fn HomeView() -> Element {
    let mut is_loading = use_signal(|| false);
    let mut status = use_signal(|| String::new());
    let mut packages = use_signal(|| Vec::<(PackageModel, PackageVersionModel, String)>::new());

    let load_packages = move || {
        spawn(async move {
            is_loading.set(true);

            let api = OnyxApi::default();
            match api.load_packages().await {
                Ok(p) => {
                    let mut a = p
                        .into_iter()
                        .map(|(p, v)| (p, v.clone(), api.version_download_url(v.id)))
                        .collect::<Vec<_>>();
                    a.sort_by(|v0, v1| v1.1.created_at.cmp(&v0.1.created_at));
                    packages.set(a);
                }
                Err(e) => status.set(format!("Error: {}", e)),
            };

            is_loading.set(false);
        });
    };

    // Fetch on mount
    use_effect(move || {
        load_packages();
    });

    rsx! {
        Header { show_auth: true },
        div {
            style: "padding: 40px; font-family: Arial, sans-serif;",

            h3 {
                "Packages in this registry"
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

            for (package, latest_version, download_url) in packages.read().iter() {
                div {
                    key: "{package.id}",
                    style: "display: flex; flex-direction: column; border-left: 1px solid black; border-bottom: 1px solid black; padding: 4px; margin-top: 4px;",
                    div {
                        "{package.name}@{latest_version.name}"
                    },
                    div {
                        "published {time_ago(latest_version.created_at)}"
                    },
                    div {
                        "blake3: {latest_version.id.to_string()}"
                    },
                    a {
                        href: "{download_url}",
                        "Download"
                    },
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

use db::PackageModel;
use dioxus::prelude::*;
use serde_json::json;

use crate::Route;

#[component]
pub fn HomeView() -> Element {
    let mut is_loading = use_signal(|| false);
    let mut status = use_signal(|| None);
    let mut packages = use_signal(|| Vec::<PackageModel>::new());

    let load_packages = move || {
        spawn(async move {
            is_loading.set(true);

            let client = reqwest::Client::new();

            match client.get("http://localhost:3000/packages").send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        status.set(None);
                        packages.set(response.json().await.unwrap())
                    } else {
                        let status_code = response.status();
                        let error_text = response.text().await.unwrap_or_default();
                        status.set(Some(format!(
                            "Failed to load packages: {} - {}",
                            status_code, error_text
                        )));
                    }
                }
                Err(e) => {
                    status.set(Some(format!("Error: {}", e)));
                }
            }

            is_loading.set(false);
        });
    };

    // Fetch on mount
    use_effect(move || {
        load_packages();
    });

    rsx! {
        div {
            style: "padding: 40px; max-width: 400px; margin: 0 auto; font-family: Arial, sans-serif;",

            h1 {
                style: "text-align: center; margin-bottom: 30px; color: #333;",
                "Noir Package Manager"
            }

            div {
                style: "display: flex; gap: 10px; margin-bottom: 20px;",

                Link { to: Route::AuthView,
                    button {
                        style: "flex: 1; padding: 12px; background-color: #007bff; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                        style: if is_loading() { "opacity: 0.6; cursor: not-allowed;" } else { "" },
                        "Auth"
                    }
                }
            }
            ul {
                    for package in packages.iter() {
                        li { key: "{package.id}", "List item: {package:?}" }
                    }
                }
        }
    }
}

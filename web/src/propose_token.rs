use dioxus::prelude::*;
use onyx_api::OnyxApi;
use web_sys::UrlSearchParams;

use super::components::Auth;
use crate::Route;

fn get_query_param(key: &str) -> String {
    let window = web_sys::window().unwrap();
    let search = window.location().search().unwrap_or_default();
    let params = UrlSearchParams::new_with_str(&search).unwrap();
    params.get(key).unwrap_or_default()
}

#[component]
pub fn ProposeTokenView() -> Element {
    let navigator = use_navigator();
    let mut is_authed = use_signal(|| false);
    let mut status_message = use_signal(|| String::new());
    let mut is_complete = use_signal(|| false);

    let handle_propose_token = move |_| {
        spawn(async move {
            let proposed_token = get_query_param("token");
            let token = crate::load_token().unwrap_or_default();

            let api = OnyxApi::default();
            match api.propose_token(proposed_token, token).await {
                Ok(()) => {
                    is_complete.set(true);
                }
                Err(e) => status_message.set(format!("Failed to activate token: {e}")),
            };
        });
    };
    rsx! {
        if *is_authed.read() {
            if *is_complete.read() {
                div {
                    style: "padding: 40px; max-width: 400px; margin: 0 auto; font-family: Arial, sans-serif;",

                    h1 {
                        style: "text-align: center; margin-bottom: 30px; color: #333;",
                        "Token activated"
                    }
                    div {
                        "You can close this page."
                    }
                }
            } else {
                div {
                    style: "padding: 40px; max-width: 400px; margin: 0 auto; font-family: Arial, sans-serif;",

                    h1 {
                        style: "text-align: center; margin-bottom: 30px; color: #333;",
                        "An application is attempting to register a token!"
                    }
                    div {
                        "If this is expected, then press continue, otherwise press cancel."
                    }

                    button {
                        onclick: handle_propose_token,
                        style: "padding: 12px; background-color: #28a745; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                        "Continue"
                    }

                    button {
                        onclick: {
                            move |_| {
                                navigator.push(Route::HomeView);
                            }
                        },
                        style: "padding: 12px; background-color: #28a745; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                        "Cancel"
                    }

                    if !status_message.read().is_empty() {
                        div {
                            style: "padding: 10px; border-radius: 4px; text-align: center; font-weight: bold;",
                            style: if status_message.read().contains("successful") {
                                "background-color: #d4edda; color: #155724; border: 1px solid #c3e6cb;"
                            } else {
                                "background-color: #f8d7da; color: #721c24; border: 1px solid #f5c6cb;"
                            },
                            "{status_message}"
                        }
                    }
                }
            }
        } else {
            Auth {
                on_auth: move |_| {
                    is_authed.set(true);
                }
            }
        }
    }
}

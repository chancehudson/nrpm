use dioxus::prelude::*;
use web_sys::UrlSearchParams;

use super::components::Auth;
use crate::Route;
use crate::components::Header;

fn get_query_param(key: &str) -> String {
    let window = web_sys::window().unwrap();
    let search = window.location().search().unwrap_or_default();
    let params = UrlSearchParams::new_with_str(&search).unwrap();
    params.get(key).unwrap_or_default()
}

#[component]
pub fn ProposeTokenView() -> Element {
    let navigator = use_navigator();

    let auth_store = &crate::AUTH_STORE;

    let mut is_authed = use_signal(|| false);
    let mut status_message = use_signal(|| String::new());
    let mut is_complete = use_signal(|| false);

    let handle_propose_token = move |_| {
        spawn(async move {
            let proposed_token = get_query_param("token");
            let self_token = {
                let auth_store = auth_store.read();
                auth_store.token.read().clone()
            };
            if self_token.is_none() {
                status_message.set(format!("Not authorized!"));
                return;
            }

            match auth_store
                .read()
                .api
                .propose_token(proposed_token, self_token.unwrap())
                .await
            {
                Ok(()) => {
                    is_complete.set(true);
                }
                Err(e) => status_message.set(format!("Failed to activate token: {e}")),
            };
        });
    };
    rsx! {
        Header { show_auth: true },
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
                        style: "display: flex; flex-direction: row; align-items: center; justify-content: center;",
                        button {
                            onclick: handle_propose_token,
                            style: "padding: 12px; background-color: #28a745; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                            "Allow"
                        }
                        div {
                            style: "width: 8px"
                        },
                        button {
                            onclick: {
                                move |_| {
                                    navigator.push(Route::HomeView);
                                }
                            },
                            style: "padding: 12px; background-color: #f87171; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                            "Abort"
                        }
                    },

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

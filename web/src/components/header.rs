use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn Header(hide_auth: bool) -> Element {
    let auth_store = &crate::AUTH_STORE;

    rsx! {
        div {
            style: "margin: 4px; padding: 4px; display: flex; flex-direction: row; justify-content: space-between; border-bottom: 1px solid black;",
            div {
                Link {
                    style: "text-decoration: none; color: inherit;",
                    to: Route::HomeView,
                    h3 {
                        "Noir Package Manager"
                    }
                }
            },
            if !hide_auth {
                div {
                    style: "display: flex; flex-direction: column; align-items: flex-end;",
                    if let Some(login) = auth_store.read().login.read().as_ref() {
                        div {
                            style: "margin-bottom: 8px;",
                            "Welcome back, {login.user.username}"
                        }
                        button {
                            style: "flex: 1; padding: 12px; background-color: #007bff; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                            onclick: {
                                move |_| {
                                    auth_store.write().clear_login();
                                }
                            },
                            "Logout"
                        }
                    } else {
                        Link { to: Route::AuthView,
                            button {
                                style: "flex: 1; padding: 12px; background-color: #007bff; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                                "Login/Signup"
                            }
                        }
                    }
                }
            }
        }
    }
}

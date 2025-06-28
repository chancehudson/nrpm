use dioxus::prelude::*;
use onyx_api::prelude::*;

use crate::Route;

#[component]
pub fn Header() -> Element {
    let mut is_authed = use_signal(|| crate::load_token());
    let mut login = use_signal(|| None);
    use_effect(move || {
        spawn(async move {
            if is_authed.read().is_some() {
                let api = OnyxApi::default();
                match api.auth(is_authed.read().as_ref().unwrap().clone()).await {
                    Ok(l) => login.set(Some(l)),
                    Err(e) => {}
                };
            }
        });
    });
    rsx! {
        div {
            style: "margin: 4px; padding: 4px; display: flex; flex-direction: row; justify-content: space-between; border-bottom: 1px solid black;",
            div {
                h3 {
                    "Noir Package Manager"
                }
            },
            div {
                style: "display: flex; flex-direction: column; align-items: flex-end;",
                if let Some(login_data) = login.read().as_ref() {
                    div {
                        "Welcome back, {login_data.user.username}"
                    }
                    button {
                        style: "flex: 1; padding: 12px; background-color: #007bff; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                        onclick: {
                            move |_| {
                                crate::remove_token();
                                is_authed.set(None);
                                login.set(None);
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

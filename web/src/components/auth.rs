use dioxus::prelude::*;

use onyx_api::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct AuthProps {
    on_auth: EventHandler<()>,
    children: Element,
}

#[component]
pub fn Auth(props: AuthProps) -> Element {
    let auth_store = &crate::AUTH_STORE;

    let mut username = use_signal(|| String::new());
    let mut password = use_signal(|| String::new());
    let status_message = use_signal(|| String::new());

    let is_loading = auth_store.read().is_loading().clone();
    let login = auth_store.read().login.read().clone();

    let handle_login = {
        move |_| {
            let username_val = username.read().clone();
            let password_val = password.read().clone();
            let mut status = status_message.clone();
            spawn(async move {
                status.set("Logging in...".to_string());

                let api = auth_store.with(|v| v.api.clone());
                match api
                    .login(LoginRequest {
                        username: username_val,
                        password: password_val,
                    })
                    .await
                {
                    Ok(login) => {
                        auth_store.with_mut(|v| v.set_login(login));
                        props.on_auth.call(());
                    }
                    Err(e) => status.set(format!("Login failed: {e}")),
                };
            });
        }
    };

    let handle_signup = {
        move |_| {
            let username_val = username.read().clone();
            let password_val = password.read().clone();
            let mut status = status_message.clone();

            spawn(async move {
                status.set("Signing up...".to_string());

                let api = auth_store.with(|v| v.api.clone());
                match api
                    .signup(LoginRequest {
                        username: username_val,
                        password: password_val,
                    })
                    .await
                {
                    Ok(login) => {
                        auth_store.with_mut(|v| v.set_login(login));
                        props.on_auth.call(());
                    }
                    Err(e) => status.set(format!("Signup failed: {e}")),
                };
            });
        }
    };
    rsx! {
        if let Some(_login) = login {
            div {
                style: "padding: 40px; max-width: 400px; margin: 0 auto; font-family: Arial, sans-serif;",

                h1 {
                    style: "text-align: center; margin-bottom: 30px; color: #333;",
                    "You are authenticated!"
                }

                div {
                    style: "display: flex; flex-direction: row; gap: 10px;",

                        button {
                            style: "padding: 12px; background-color: #007bff; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                            onclick: {
                                move |_| {
                                    props.on_auth.call(());
                                }
                            },
                            "Continue"
                        }

                    button {
                        onclick: {
                            move |_| {
                                auth_store.write().clear_login();
                            }
                        },
                        style: "padding: 12px; background-color: #28a745; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                        "Logout"
                    }
                }
            }
        } else {
            div {
                style: "padding: 40px; max-width: 400px; margin: 0 auto; font-family: Arial, sans-serif;",

                h1 {
                    style: "text-align: center; margin-bottom: 30px; color: #333;",
                    "Login / Signup"
                }

                div {
                    style: "margin-bottom: 20px;",
                    label {
                        style: "display: block; margin-bottom: 5px; font-weight: bold; color: #555;",
                        "Username:"
                    }
                    input {
                        r#type: "text",
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                        style: "width: 100%; padding: 10px; border: 1px solid #ddd; border-radius: 4px; font-size: 16px;",
                        placeholder: "Enter your username"
                    }
                }

                div {
                    style: "margin-bottom: 30px;",
                    label {
                        style: "display: block; margin-bottom: 5px; font-weight: bold; color: #555;",
                        "Password:"
                    }
                    input {
                        r#type: "password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                        style: "width: 100%; padding: 10px; border: 1px solid #ddd; border-radius: 4px; font-size: 16px;",
                        placeholder: "Enter your password"
                    }
                }

                div {
                    style: "display: flex; gap: 10px; margin-bottom: 20px;",

                    button {
                        onclick: handle_login,
                        disabled: is_loading,
                        style: "flex: 1; padding: 12px; background-color: #007bff; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                        style: if is_loading { "opacity: 0.6; cursor: not-allowed;" } else { "" },
                        "Login"
                    }

                    button {
                        onclick: handle_signup,
                        disabled: is_loading,
                        style: "flex: 1; padding: 12px; background-color: #28a745; color: white; border: none; border-radius: 4px; font-size: 16px; cursor: pointer; transition: background-color 0.2s;",
                        style: if is_loading { "opacity: 0.6; cursor: not-allowed;" } else { "" },
                        "Signup"
                    }
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
    }
}

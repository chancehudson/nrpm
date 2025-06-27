use dioxus::prelude::*;
use gloo_storage::LocalStorage;
use gloo_storage::Storage;

mod auth;
mod components;
mod home;
mod propose_token;

use auth::AuthView;
use home::HomeView;
use propose_token::ProposeTokenView;

#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    HomeView,
    #[route("/auth")]
    AuthView,
    #[route("/propose_token")]
    ProposeTokenView,
}

fn app() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

fn main() {
    launch(app);
}

pub fn save_token(token: &str) -> Result<(), gloo_storage::errors::StorageError> {
    LocalStorage::set("auth_token", token)
}

pub fn load_token() -> Option<String> {
    LocalStorage::get("auth_token").ok()
}

pub fn remove_token() {
    LocalStorage::delete("auth_token");
}

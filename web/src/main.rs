use dioxus::prelude::*;

mod auth;
mod components;
mod home;
mod package;
mod propose_token;
mod stores;

use auth::AuthView;
use home::HomeView;
use package::PackageView;
use propose_token::ProposeTokenView;

use stores::*;

#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    HomeView,
    #[route("/_/auth")]
    AuthView,
    #[route("/_/propose_token")]
    ProposeTokenView,
    #[route("/:package_name")]
    PackageView { package_name: String },
}

fn app() -> Element {
    rsx! {
        div {
            style: "font-family: sans-serif; margin: auto; display: flex; flex-direction: column; max-width: 800px;",
            Router::<Route> {}
        }
    }
}

fn main() {
    gloo_utils::document().set_title("Noir Package Manager");
    launch(app);
}

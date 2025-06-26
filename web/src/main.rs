use dioxus::prelude::*;

mod auth;
mod home;

use auth::AuthView;
use home::HomeView;

#[derive(Routable, Clone, PartialEq)]
enum Route {
    #[route("/")]
    HomeView,
    #[route("/auth")]
    AuthView,
}

fn app() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

fn main() {
    launch(app);
}

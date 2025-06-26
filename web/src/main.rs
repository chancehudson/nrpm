use dioxus::prelude::*;

mod auth;

fn main() {
    launch(auth::App);
}

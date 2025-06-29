use dioxus::prelude::*;

use super::components::Auth;
use super::components::Header;
use crate::Route;

#[component]
pub fn AuthView() -> Element {
    let navigator = use_navigator();
    rsx! {
        Header { hide_auth: true },
        Auth {
            on_auth: move |_| {
                navigator.push(Route::HomeView);
            }
        }
    }
}

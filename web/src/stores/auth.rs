use dioxus::prelude::*;
use gloo_storage::LocalStorage;
use gloo_storage::Storage;

use onyx_api::prelude::*;

pub static AUTH_STORE: GlobalSignal<AuthStore> = Signal::global(AuthStore::new);

const AUTH_TOKEN_LOCALSTORAGE: &'static str = "auth_token";

#[derive(Clone, Debug)]
pub struct AuthStore {
    pub login: Signal<Option<LoginResponse>>,
    pub token: Signal<Option<String>>,
    pub api: OnyxApi,
}

impl AuthStore {
    pub fn new() -> Self {
        let token = LocalStorage::get(AUTH_TOKEN_LOCALSTORAGE).ok();
        let mut out = Self {
            login: Signal::new(None),
            token: Signal::new(token.clone()),
            api: OnyxApi::default(),
        };
        if token.is_some() {
            out.load_login();
        }
        out
    }

    /// The store should either use a token to load a login, or have
    /// no token and no login
    pub fn is_loading(&self) -> bool {
        self.token.read().is_some() && self.login.read().is_none()
    }

    pub fn set_login(&mut self, login: LoginResponse) {
        LocalStorage::set(AUTH_TOKEN_LOCALSTORAGE, login.token.clone()).unwrap();
        self.token.with_mut(|v| *v = Some(login.token.clone()));
        self.login.with_mut(|v| *v = Some(login));
    }

    pub fn clear_login(&mut self) {
        LocalStorage::delete(AUTH_TOKEN_LOCALSTORAGE);
        self.token.with_mut(|v| *v = None);
        self.login.with_mut(|v| *v = None);
    }

    pub fn load_login(&mut self) {
        #[cfg(debug_assertions)]
        if self.is_loading() {
            println!("WARNING: Loading login twice!")
        }

        let token_maybe = self.token.with(|v| v.clone());
        if token_maybe.is_none() {
            self.login.with_mut(|v| *v = None);
            return;
        }
        let mut self_clone = self.clone();
        spawn(async move {
            let token = self_clone.token.with(|v| v.clone()).unwrap();
            match self_clone.api.auth(token).await {
                Ok(login) => self_clone.set_login(login),
                Err(e) => {
                    println!("auth error: {:?}", e);
                    self_clone.clear_login();
                }
            };
        });
    }
}

use leptos::prelude::*;
use shared::dto::user::UserDto;

#[derive(Clone, Copy)]
pub struct AuthState {
    pub user: RwSignal<Option<UserDto>>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            user: RwSignal::new(None),
        }
    }

    pub fn set_user(&self, user: UserDto) {
        self.user.set(Some(user));
    }

    pub fn clear(&self) {
        self.user.set(None);
    }

    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.user.with(Option::is_some)
    }
}

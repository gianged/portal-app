//! Chat UI state that outlives the chat page, so the selected channel survives
//! navigating away and back. Provided by the authed layout; dropped on logout.

use leptos::prelude::*;

use shared::dto::ids::ChannelId;

#[derive(Clone, Copy)]
pub struct ChatUiState {
    pub selected_channel: RwSignal<Option<ChannelId>>,
}

impl Default for ChatUiState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatUiState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected_channel: RwSignal::new(None),
        }
    }
}

//! Chat surface: the channel rail beside the open channel's thread and composer; owns the selected channel and its message/typing state.

use leptos::prelude::*;

use shared::dto::chat::MessageDto;

use crate::features::chat::channel_list::ChannelList;
use crate::features::chat::composer::Composer;
use crate::features::chat::message_thread::MessageThread;
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::IconName;
use crate::state::chat::ChatUiState;
use crate::theme::{self, color, radius, space};

#[component]
pub fn ChatView() -> impl IntoView {
    // Layout-owned selection, so the open channel survives leaving and re-entering /chat.
    let channel = use_context::<ChatUiState>()
        .expect("ChatUiState context")
        .selected_channel;
    let messages: RwSignal<Vec<MessageDto>> = RwSignal::new(Vec::new());
    let typing = RwSignal::new(false);
    // Presence-only memo keeps the thread and composer mounted across channel switches.
    let has_channel = Memo::new(move |_| channel.get().is_some());

    let frame = theme::class(format!(
        "display: flex; height: calc(100vh - 180px); min-height: 420px; \
         border: 1px solid {b}; border-radius: {r}; overflow: hidden; background: {bg};",
        b = color::BORDER,
        r = radius::LG,
        bg = color::BG_ELEVATED,
    ));
    let left = theme::class(format!(
        "width: 260px; flex-shrink: 0; border-right: 1px solid {b}; overflow-y: auto; background: {bg};",
        b = color::BORDER,
        bg = color::BG_SUBTLE,
    ));
    let right = theme::class("flex: 1; min-width: 0; display: flex; flex-direction: column;");
    let empty_wrap = theme::class(format!(
        "flex: 1; display: flex; align-items: center; justify-content: center; padding: {p};",
        p = space::D6
    ));

    view! {
        <div class=frame>
            <div class=left>
                <ChannelList selected=channel />
            </div>
            <div class=right>
                {move || {
                    if has_channel.get() {
                        view! {
                            <MessageThread channel=channel messages=messages typing=typing />
                            <Composer channel=channel messages=messages />
                        }.into_any()
                    } else {
                        view! {
                            <div class=empty_wrap.clone()>
                                <EmptyState
                                    icon=IconName::Chat
                                    title="Pick a channel"
                                    description="Choose a conversation on the left, or start a DM."
                                />
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

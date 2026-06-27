use leptos::prelude::*;

use crate::primitives::icon::{Icon, IconName};
use crate::state::toast::{ToastKind, ToastState};
use crate::theme::{self, color, radius, space, typography};

/// Renders the toast stack from [`ToastState`] in the bottom-right corner.
/// Mounted once at the app root, above the routed content.
#[component]
pub fn ToastHost() -> impl IntoView {
    let toasts = use_context::<ToastState>().expect("ToastState context");
    let host = theme::class(format!(
        "position: fixed; bottom: {b}; right: {b}; z-index: 200; \
         display: flex; flex-direction: column; gap: {g}; max-width: 360px;",
        b = space::D5,
        g = space::D2,
    ));
    view! {
        <div class=host>
            <For
                each=move || toasts.items.get()
                key=|t| t.id
                let:toast
            >
                {
                    let (bg, fg, border) = match toast.kind {
                        ToastKind::Success => (color::SUCCESS_BG, color::SUCCESS, color::SUCCESS_BORDER),
                        ToastKind::Error => (color::DANGER_BG, color::DANGER, color::DANGER_BORDER),
                    };
                    let cls = theme::class(format!(
                        "display: flex; align-items: flex-start; gap: {g}; padding: {py} {px}; \
                         background: {bg}; color: {fg}; border: 1px solid {border}; \
                         border-radius: {r}; box-shadow: {s}; font-family: {ff}; font-size: {fs}; \
                         line-height: 1.4;",
                        g = space::D2,
                        py = space::D3,
                        px = space::D4,
                        r = radius::MD,
                        s = typography::SHADOW_MD,
                        ff = typography::FONT_SANS,
                        fs = typography::TEXT_SMALL,
                    ));
                    let body = theme::class("flex: 1; min-width: 0; display: flex; \
                                      flex-direction: column; word-wrap: break-word;");
                    let title_cls = theme::class(format!(
                        "font-weight: {fw}; margin-bottom: {mb};",
                        fw = typography::WEIGHT_SEMIBOLD,
                        mb = space::D1,
                    ));
                    let close = theme::class("background: transparent; border: none; color: inherit; \
                                       cursor: pointer; opacity: 0.7; display: inline-flex; \
                                       padding: 0; &:hover { opacity: 1; }");
                    let id = toast.id;
                    let icon = match toast.kind {
                        ToastKind::Success => IconName::Check,
                        ToastKind::Error => IconName::AlertCircle,
                    };
                    view! {
                        <div class=cls role="status">
                            <Icon name=icon size=16 />
                            <div class=body>
                                {toast.title.map(|t| view! { <div class=title_cls>{t}</div> })}
                                <span>{toast.message}</span>
                            </div>
                            <button class=close on:click=move |_| toasts.dismiss(id)>
                                <Icon name=IconName::Close size=14 />
                            </button>
                        </div>
                    }
                }
            </For>
        </div>
    }
}

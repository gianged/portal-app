use leptos::prelude::*;

use crate::primitives::button::{Button, ButtonVariant};
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::theme::{self, color, typography};

/// Confirmation gate for destructive actions. Confirming runs `on_confirm`
/// first, then `on_close`; the parent owns the open state and the action.
#[component]
pub fn ConfirmDialog(
    #[prop(into)] open: Signal<bool>,
    #[prop(into)] title: String,
    #[prop(into)] message: Signal<String>,
    #[prop(optional, into)] confirm_label: Option<String>,
    on_confirm: Callback<()>,
    on_close: Callback<()>,
) -> impl IntoView {
    // Dialog children are re-callable, so everything captured must be Copy.
    let msg_cls = StoredValue::new(theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; margin: 0; line-height: 1.5;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    )));
    let title = StoredValue::new(title);
    let confirm_label = StoredValue::new(confirm_label.unwrap_or_else(|| "Confirm".to_owned()));
    let cancel = Callback::new(move |_| on_close.run(()));
    let confirm = Callback::new(move |_| {
        on_confirm.run(());
        on_close.run(());
    });
    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title=title.get_value() />
            <DialogBody>
                <p class=msg_cls.get_value()>{move || message.get()}</p>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Destructive on_click=confirm>
                    {confirm_label.get_value()}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

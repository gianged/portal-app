//! Small presentational helpers shared across feature pages: headings, muted captions, detail-page building blocks (description, progress), and the back link; scoped classes over the design tokens.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::chart::ProgressBar;
use crate::primitives::cluster::Cluster;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::Input;
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{self, color, typography};

/// Card / section heading (16px semibold strong).
#[must_use]
pub fn section_heading(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <h3 class=cls>{text.to_owned()}</h3> }.into_any()
}

/// Page title (20px semibold strong).
#[must_use]
pub fn page_title(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0; \
         letter-spacing: -0.015em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H2,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <h2 class=cls>{text.to_owned()}</h2> }.into_any()
}

/// Muted caption / secondary line.
#[must_use]
pub fn subtle(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}

/// Uppercase eyebrow label above a section or form group.
#[must_use]
pub fn eyebrow_title(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-size: {fs}; font-weight: {fw}; color: {c}; text-transform: uppercase; \
         letter-spacing: 0.04em;",
        fs = typography::TEXT_LABEL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}

/// Class for muted small body text.
#[must_use]
pub fn muted_class() -> String {
    theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ))
}

/// Class for semibold strong inline text.
#[must_use]
pub fn strong_class() -> String {
    theme::class(format!(
        "font-family: {ff}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ))
}

/// Pre-wrapped long-form description paragraph on detail pages.
#[must_use]
pub fn desc_block(description: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.55; white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    view! { <p class=cls>{description.to_owned()}</p> }.into_any()
}

/// "Progress N%" label with a compact bar, for detail-page headers.
#[must_use]
pub fn progress_row(progress: u8) -> AnyView {
    let wrap = theme::class("display: flex; align-items: center; gap: 12px;");
    let label = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; white-space: nowrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let bar = theme::class("flex: 1; max-width: 260px;");
    view! {
        <div class=wrap>
            <span class=label>{format!("Progress {progress}%")}</span>
            <div class=bar>
                <ProgressBar value=Signal::derive(move || progress) />
            </div>
        </div>
    }
    .into_any()
}

/// Progress editor card: validates a whole 0-100 input, then hands the value
/// to `on_save`; the caller owns the mutation and the `saving` flag.
#[component]
pub fn ProgressEditor(
    initial: u8,
    on_save: Callback<u8>,
    #[prop(into)] saving: Signal<bool>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let value = RwSignal::new(initial.to_string());
    let on_input = Callback::new(move |v: String| value.set(v));
    let save = Callback::new(move |_| {
        if saving.get_untracked() {
            return;
        }
        let Ok(p) = value.get_untracked().trim().parse::<u8>() else {
            toast.error("Progress must be a whole number between 0 and 100.");
            return;
        };
        if p > 100 {
            toast.error("Progress must be between 0 and 100.");
            return;
        }
        on_save.run(p);
    });
    let input_wrap = theme::class("width: 110px;");
    view! {
        <Card>
            <Stack gap=Gap::Sm>
                {section_heading("Set progress")}
                <Cluster gap=Gap::Sm>
                    <div class=input_wrap>
                        <Input value=value on_input=on_input placeholder="0-100" />
                    </div>
                    <Button
                        variant=ButtonVariant::Primary
                        size=ButtonSize::Sm
                        on_click=save
                        disabled=saving
                    >
                        "Save"
                    </Button>
                </Cluster>
            </Stack>
        </Card>
    }
}

/// Back link with a leading chevron, for detail pages.
#[must_use]
pub fn back_link(href: &'static str, label: &str) -> AnyView {
    let cls = theme::class(format!(
        "display: inline-flex; align-items: center; gap: 4px; font-family: {ff}; \
         font-size: {fs}; color: {c}; text-decoration: none; &:hover {{ color: {a}; }}",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
        a = color::ACCENT,
    ));
    let label = label.to_owned();
    view! {
        <A href=href attr:class=cls>
            <Icon name=IconName::ChevronLeft size=14 /> {label}
        </A>
    }
    .into_any()
}

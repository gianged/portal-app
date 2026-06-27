use leptos::prelude::*;

use crate::theme::{self, color, typography};

#[derive(Clone, Copy, Default)]
pub enum AvatarSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl AvatarSize {
    fn dimension(self) -> &'static str {
        match self {
            Self::Sm => "24px",
            Self::Md => "32px",
            Self::Lg => "40px",
        }
    }

    fn font_size(self) -> &'static str {
        match self {
            Self::Sm => "11px",
            Self::Md => "12.5px",
            Self::Lg => "14px",
        }
    }
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .collect::<String>()
        .to_uppercase()
}

#[component]
pub fn Avatar(
    #[prop(into)] name: String,
    #[prop(optional)] tone: usize,
    #[prop(optional)] size: AvatarSize,
) -> impl IntoView {
    let (bg, fg) = color::AVATAR_TONES[tone % color::AVATAR_TONES.len()];
    let initials = initials(&name);
    let dim = size.dimension();
    let fs = size.font_size();
    let cls = theme::class(format!(
        "display: inline-flex; align-items: center; justify-content: center; \
         width: {dim}; height: {dim}; border-radius: 50%; \
         background: {bg}; color: {fg}; \
         font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         flex-shrink: 0; user-select: none;",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
    ));
    view! { <span class=cls title=name>{initials}</span> }
}

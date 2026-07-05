#![allow(dead_code)]

use leptos::prelude::*;

use crate::theme::{self, color, radius, space};

#[derive(Clone, Copy, Default)]
pub enum Pad {
    None,
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl Pad {
    fn value(self) -> &'static str {
        match self {
            Self::None => "0",
            Self::Xs => space::D2,
            Self::Sm => space::D3,
            Self::Md => space::D4,
            Self::Lg => space::D5,
            Self::Xl => space::D6,
        }
    }
}

#[component]
pub fn Box_(
    #[prop(optional)] pad: Pad,
    #[prop(optional)] bordered: bool,
    #[prop(optional, into)] background: Option<String>,
    children: Children,
) -> impl IntoView {
    let bg = background.unwrap_or_else(|| color::BG.to_string());
    let border = if bordered {
        format!("1px solid {}", color::BORDER)
    } else {
        "none".to_string()
    };
    let cls = theme::class(format!(
        "padding: {p}; background: {bg}; border: {border}; border-radius: {r};",
        p = pad.value(),
        r = radius::MD,
    ));
    view! { <div class=cls>{children()}</div> }
}

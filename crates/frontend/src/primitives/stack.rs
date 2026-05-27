use leptos::prelude::*;

use crate::theme::{
    class,
    space::{D1, D2, D3, D5, D7},
};

#[derive(Clone, Copy, Default)]
pub enum Gap {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl Gap {
    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::Xs => D1,
            Self::Sm => D2,
            Self::Md => D3,
            Self::Lg => D5,
            Self::Xl => D7,
        }
    }
}

#[component]
pub fn Stack(
    #[prop(optional)] gap: Gap,
    #[prop(optional, into)] align: Option<String>,
    children: Children,
) -> impl IntoView {
    let align = align.unwrap_or_else(|| "stretch".to_string());
    let cls = class(format!(
        "display: flex; flex-direction: column; gap: {g}; align-items: {a}; min-width: 0;",
        g = gap.value(),
        a = align,
    ));
    view! { <div class=cls>{children()}</div> }
}

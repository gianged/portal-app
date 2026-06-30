use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::theme::{self, color, radius, space, typography};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
    Destructive,
    #[allow(dead_code)] // TODO: unused
    Link,
    Icon,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl ButtonSize {
    fn height(self) -> &'static str {
        match self {
            Self::Sm => space::BTN_H_SM,
            Self::Md => space::BTN_H,
            Self::Lg => space::BTN_H_LG,
        }
    }

    fn padding_x(self) -> &'static str {
        match self {
            Self::Sm => space::D3,
            Self::Md => space::D4,
            Self::Lg => space::D5,
        }
    }

    fn font_size(self) -> &'static str {
        match self {
            Self::Sm => typography::TEXT_CAPTION,
            Self::Md => typography::TEXT_SMALL,
            Self::Lg => "14.5px",
        }
    }
}

fn variant_css(v: ButtonVariant) -> String {
    match v {
        ButtonVariant::Primary => format!(
            "background: {a}; color: {fg}; border-color: {a}; box-shadow: {s}, inset 0 1px 0 rgba(255,255,255,0.12); \
             &:hover:not(:disabled) {{ background: {ah}; border-color: {ah}; }} \
             &:active:not(:disabled) {{ background: {aa}; }}",
            a = color::ACCENT,
            fg = color::TEXT_ON_ACCENT,
            ah = color::ACCENT_HOVER,
            aa = color::ACCENT_ACTIVE,
            s = typography::SHADOW_XS,
        ),
        ButtonVariant::Secondary => format!(
            "background: {bg}; color: {fg}; border-color: {b}; box-shadow: {s}; \
             &:hover:not(:disabled) {{ background: {bh}; border-color: {bs}; }} \
             &:active:not(:disabled) {{ background: {ba}; }}",
            bg = color::BG_ELEVATED,
            fg = color::TEXT,
            b = color::BORDER,
            bh = color::BG_HOVER,
            bs = color::BORDER_STRONG,
            ba = color::BG_ACTIVE,
            s = typography::SHADOW_XS,
        ),
        ButtonVariant::Ghost => format!(
            "background: transparent; color: {fg}; \
             &:hover:not(:disabled) {{ background: {bh}; }} \
             &:active:not(:disabled) {{ background: {ba}; }}",
            fg = color::TEXT,
            bh = color::BG_HOVER,
            ba = color::BG_ACTIVE,
        ),
        ButtonVariant::Destructive => format!(
            "background: {d}; color: #fff; border-color: {d}; box-shadow: {s}, inset 0 1px 0 rgba(255,255,255,0.12); \
             &:hover:not(:disabled) {{ background: {dh}; border-color: {dh}; }}",
            d = color::DANGER,
            dh = color::DANGER_HOVER,
            s = typography::SHADOW_XS,
        ),
        ButtonVariant::Link => format!(
            "background: transparent; color: {a}; padding: 0; height: auto; \
             &:hover:not(:disabled) {{ color: {ah}; text-decoration: underline; text-underline-offset: 3px; }}",
            a = color::ACCENT,
            ah = color::ACCENT_HOVER,
        ),
        ButtonVariant::Icon => format!(
            "background: transparent; color: {fg}; padding: 0; \
             &:hover:not(:disabled) {{ color: {ft}; background: {bh}; }}",
            fg = color::TEXT_MUTED,
            ft = color::TEXT,
            bh = color::BG_HOVER,
        ),
    }
}

#[component]
pub fn Button(
    #[prop(optional)] variant: ButtonVariant,
    #[prop(optional)] size: ButtonSize,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] type_: Option<String>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    #[prop(optional)] full_width: bool,
    children: Children,
) -> impl IntoView {
    let h = size.height();
    let px = size.padding_x();
    let fs = size.font_size();
    let r = if matches!(size, ButtonSize::Sm) {
        radius::SM
    } else {
        radius::MD
    };
    let w = if full_width { "100%" } else { "auto" };
    let icon_w = if matches!(variant, ButtonVariant::Icon) {
        format!("width: {h};")
    } else {
        String::new()
    };

    let base = format!(
        "display: inline-flex; align-items: center; justify-content: center; \
         gap: {g}; height: {h}; padding: 0 {px}; border-radius: {r}; \
         border: 1px solid transparent; font-family: {ff}; font-weight: {fw}; \
         font-size: {fs}; letter-spacing: -0.005em; white-space: nowrap; \
         cursor: pointer; user-select: none; width: {w}; {icon_w} \
         transition: background 120ms ease, border-color 120ms ease, color 120ms ease, box-shadow 120ms ease; \
         &:focus-visible {{ outline: none; box-shadow: {ring}; }} \
         &:disabled {{ opacity: 0.5; cursor: not-allowed; }}",
        g = space::D2,
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_MEDIUM,
        ring = typography::RING,
    );

    let cls = theme::class(format!("{base} {}", variant_css(variant)));
    let type_ = type_.unwrap_or_else(|| "button".to_string());
    let on_click_handler = move |ev: MouseEvent| {
        if let Some(cb) = on_click {
            cb.run(ev);
        }
    };

    view! {
        <button class=cls type=type_ disabled=move || disabled.get() on:click=on_click_handler>
            {children()}
        </button>
    }
}

use leptos::prelude::*;

use crate::theme::{class, color, radius, space, typography};

/// Bordered, rounded container for a [`Table`] (and optional [`TableToolbar`] /
/// footer). Clips the table's corners via `overflow: hidden`.
#[component]
pub fn TableWrap(children: Children) -> impl IntoView {
    let cls = class(format!(
        "background: {bg}; border: 1px solid {b}; border-radius: {r}; \
         overflow: hidden; box-shadow: {s};",
        bg = color::BG_ELEVATED,
        b = color::BORDER,
        r = radius::LG,
        s = typography::SHADOW_XS,
    ));
    view! { <div class=cls>{children()}</div> }
}

/// Header strip above a table — title/filters on the left, actions on the right.
#[component]
pub fn TableToolbar(children: Children) -> impl IntoView {
    let cls = class(format!(
        "display: flex; align-items: center; justify-content: space-between; gap: {g}; \
         padding: {py} {px}; border-bottom: 1px solid {b}; background: {bg};",
        g = space::D3,
        py = space::D3,
        px = space::D4,
        b = color::BORDER,
        bg = color::BG_ELEVATED,
    ));
    view! { <div class=cls>{children()}</div> }
}

/// The `<table>` itself. Styles `thead`/`tbody` via descendant selectors, so
/// callers write plain `<thead>/<tbody>/<tr>/<th>/<td>` inside. Helper cell
/// classes are available on descendants: `cell-strong`, `cell-muted`, `mono`.
#[component]
pub fn Table(children: Children) -> impl IntoView {
    let cls = class(format!(
        "width: 100%; border-collapse: collapse; \
         & thead th {{ text-align: left; font-family: {ff}; font-weight: {fw}; font-size: 12px; \
            color: {tm}; text-transform: uppercase; letter-spacing: 0.04em; padding: {py} {px}; \
            background: {sub}; border-bottom: 1px solid {b}; white-space: nowrap; }} \
         & tbody td {{ padding: {py} {px}; border-bottom: 1px solid {b}; font-size: {fs}; \
            color: {tx}; vertical-align: middle; height: {rh}; }} \
         & tbody tr:last-child td {{ border-bottom: none; }} \
         & tbody tr:hover {{ background: {sub}; }} \
         & .cell-strong {{ font-weight: {fw}; color: {ts}; }} \
         & .cell-muted {{ color: {tm}; }} \
         & .mono {{ font-family: {fm}; }}",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_MEDIUM,
        tm = color::TEXT_MUTED,
        py = space::D3,
        px = space::D4,
        sub = color::BG_SUBTLE,
        b = color::BORDER,
        fs = typography::TEXT_SMALL,
        tx = color::TEXT,
        rh = space::ROW_H,
        ts = color::TEXT_STRONG,
        fm = typography::FONT_MONO,
    ));
    view! { <table class=cls>{children()}</table> }
}

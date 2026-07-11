use leptos::prelude::*;

use shared::dto::report::ReportSummaryDto;

use crate::features::reports::api;
use crate::features::reports::components::section_title;
use crate::primitives::card::Card;
use crate::primitives::stack::{Gap, Stack};
use crate::theme::{self, color, space, typography};
use crate::util::load::{self, Loadable};

/// Generated-PDF archive; `refresh` re-fetches after a new report is generated.
#[component]
pub fn ReportArchive(#[prop(into)] refresh: Signal<u32>) -> impl IntoView {
    let items: Loadable<Vec<ReportSummaryDto>> = Loadable::new();

    Effect::new(move |_| {
        let _ = refresh.get();
        load::load(items, api::archive_list());
    });

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                {section_title("Archive")}
                {move || match items.get() {
                    None => load::note("Loading…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => load::note("No reports generated yet."),
                    Some(Ok(list)) => list.into_iter().map(archive_row).collect_view().into_any(),
                }}
            </Stack>
        </Card>
    }
}

fn archive_row(r: ReportSummaryDto) -> impl IntoView {
    let row = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
        g = space::D3,
        p = space::D2,
        b = color::BORDER,
    ));
    let label = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT_STRONG,
    ));
    let meta = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let link = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; font-weight: {fw};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::ACCENT,
        fw = typography::WEIGHT_MEDIUM,
    ));
    let grow = theme::class("flex: 1; min-width: 0;");

    let period = format!(
        "{}-{:02}",
        r.period_start.year(),
        u8::from(r.period_start.month())
    );
    let kb = (r.size_bytes / 1024).max(1);

    view! {
        <div class=row>
            <span class=label>{format!("{} · {period}", r.kind.label())}</span>
            <span class=grow></span>
            <span class=meta>{format!("{kb} KB")}</span>
            <a class=link href=r.download_url target="_blank" rel="noopener">"Download"</a>
        </div>
    }
}

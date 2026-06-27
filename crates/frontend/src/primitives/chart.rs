//! Hand-built SVG charts: zero-dependency, WASM-safe, and themeable via design-token
//! CSS variables for fill/stroke, so they reskin on the `data-theme` flip. Data is
//! passed in as concrete `Vec`s and rendered once; only [`ProgressBar`] is reactive.

use leptos::prelude::*;

use crate::theme::{class, color, typography};

/// SVG user-units; the chart scales to its container via `viewBox` + `width:100%`.
const VB_W: f64 = 640.0;
const PAD: f64 = 30.0;

/// One line in a [`LineChart`].
#[derive(Clone)]
pub struct Series {
    pub label: String,
    pub points: Vec<f64>,
    pub color: &'static str,
}

/// Theme palette for chart series, cycled by index.
#[must_use]
pub fn series_color(i: usize) -> &'static str {
    const PALETTE: [&str; 5] = [
        color::ACCENT,
        color::SUCCESS,
        color::WARNING,
        color::DANGER,
        color::INFO,
    ];
    PALETTE[i % PALETTE.len()]
}

fn n(v: f64) -> String {
    format!("{v:.1}")
}

#[component]
pub fn LineChart(
    series: Vec<Series>,
    x_labels: Vec<String>,
    #[prop(optional)] height: Option<u32>,
) -> impl IntoView {
    let h = f64::from(height.unwrap_or(220));
    let count = x_labels.len().max(2);
    let max = series
        .iter()
        .flat_map(|s| s.points.iter().copied())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let step = (VB_W - 2.0 * PAD) / (count as f64 - 1.0);
    let xf = move |i: usize| PAD + i as f64 * step;
    let yf = move |v: f64| h - PAD - (v.max(0.0) / max) * (h - 2.0 * PAD);
    let axis_y = h - PAD;

    let lines = series
        .iter()
        .map(|s| {
            let pts = s
                .points
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{:.1},{:.1}", xf(i), yf(*v)))
                .collect::<Vec<_>>()
                .join(" ");
            let stroke = s.color;
            view! { <polyline fill="none" stroke=stroke stroke-width="2" points=pts /> }
        })
        .collect_view();

    let labels = x_labels
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(i, l)| {
            view! {
                <text x=n(xf(i)) y=n(h - 8.0) text-anchor="middle" font-size="11" fill=color::TEXT_MUTED>
                    {l.clone()}
                </text>
            }
        })
        .collect_view();

    let legend_text = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let legend_item =
        class("display: inline-flex; align-items: center; gap: 6px; margin-right: 14px;");
    let legend = series
        .iter()
        .filter(|s| !s.label.is_empty())
        .map(|s| {
            let dot = class(format!(
                "display: inline-block; width: 10px; height: 10px; border-radius: 2px; background: {};",
                s.color
            ));
            let label = s.label.clone();
            let item = legend_item.clone();
            let text = legend_text.clone();
            view! {
                <span class=item>
                    <span class=dot></span>
                    <span class=text>{label}</span>
                </span>
            }
        })
        .collect_view();

    let svg_cls = class("width: 100%; height: auto; display: block;");
    let legend_wrap = class("margin-top: 6px;");
    let vb = format!("0 0 {VB_W} {h}");
    view! {
        <div>
            <svg class=svg_cls viewBox=vb preserveAspectRatio="xMidYMid meet">
                <line x1=n(PAD) y1=n(axis_y) x2=n(VB_W - PAD) y2=n(axis_y) stroke=color::BORDER stroke-width="1" />
                <line x1=n(PAD) y1=n(axis_y) x2=n(PAD) y2=n(PAD) stroke=color::BORDER stroke-width="1" />
                {lines}
                {labels}
            </svg>
            <div class=legend_wrap>{legend}</div>
        </div>
    }
}

#[component]
pub fn BarChart(data: Vec<(String, f64)>, #[prop(optional)] height: Option<u32>) -> impl IntoView {
    let h = f64::from(height.unwrap_or(200));
    let max = data
        .iter()
        .map(|(_, v)| *v)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let count = data.len().max(1);
    let slot = (VB_W - 2.0 * PAD) / count as f64;
    let bar_w = slot * 0.55;
    let axis_y = h - PAD;

    let bars = data
        .iter()
        .enumerate()
        .map(|(i, (label, v))| {
            let bx = PAD + i as f64 * slot + (slot - bar_w) / 2.0;
            let bh = (v.max(0.0) / max) * (h - 2.0 * PAD);
            let by = axis_y - bh;
            let cx = PAD + i as f64 * slot + slot / 2.0;
            view! {
                <rect x=n(bx) y=n(by) width=n(bar_w) height=n(bh) rx="2" fill=color::ACCENT />
                <text x=n(cx) y=n(by - 3.0) text-anchor="middle" font-size="10" fill=color::TEXT>
                    {fmt_int(*v)}
                </text>
                <text x=n(cx) y=n(h - 8.0) text-anchor="middle" font-size="9" fill=color::TEXT_MUTED>
                    {truncate(label, 10)}
                </text>
            }
        })
        .collect_view();

    let svg_cls = class("width: 100%; height: auto; display: block;");
    let vb = format!("0 0 {VB_W} {h}");
    view! {
        <svg class=svg_cls viewBox=vb preserveAspectRatio="xMidYMid meet">
            <line x1=n(PAD) y1=n(axis_y) x2=n(VB_W - PAD) y2=n(axis_y) stroke=color::BORDER stroke-width="1" />
            {bars}
        </svg>
    }
}

/// A slim 0-100 progress bar. Reactive on `value`.
#[component]
pub fn ProgressBar(#[prop(into)] value: Signal<u8>) -> impl IntoView {
    let cls = class("width: 100%; height: 8px; display: block;");
    let width = move || format!("{}", value.get().min(100));
    view! {
        <svg class=cls viewBox="0 0 100 8" preserveAspectRatio="none">
            <rect x="0" y="0" width="100" height="8" rx="4" fill=color::BG_SUNKEN />
            <rect x="0" y="0" width=width height="8" rx="4" fill=color::ACCENT />
        </svg>
    }
}

fn fmt_int(v: f64) -> String {
    format!("{}", v.round() as i64)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

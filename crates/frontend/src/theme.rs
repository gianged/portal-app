pub mod color;
pub mod radius;
pub mod space;
pub mod typography;

use std::str::FromStr;

#[must_use]
pub fn class(css: impl AsRef<str>) -> String {
    // CSS comes from static templates and theme constants we control, so a parse failure is a programmer bug.
    let sheet =
        stylist::ast::Sheet::from_str(css.as_ref()).expect("CSS template is statically valid");
    stylist::Style::new(sheet)
        .expect("CSS template is statically valid")
        .get_class_name()
        .to_string()
}

/// The single global stylesheet: theme variables (light + dark), base resets, and
/// scrollbar styling, injected once at the app root. Every `color::*`/shadow/ring
/// token resolves to one of these variables, so a `data-theme` flip reskins the app.
#[must_use]
pub fn global_stylesheet() -> &'static str {
    r#"
:root {
  color-scheme: light;
  --bg: #ffffff;
  --bg-subtle: #fafbfc;
  --bg-elevated: #ffffff;
  --bg-sunken: #f4f5f7;
  --bg-hover: #f4f5f7;
  --bg-active: #ebedef;

  --text: #0a1f44;
  --text-strong: #060d1f;
  --text-muted: #51607a;
  --text-faint: #8794ab;
  --text-on-accent: #ffffff;

  --border: #e3e8ee;
  --border-strong: #cbd2dc;
  --border-focus: #2563eb;

  --accent: #2563eb;
  --accent-hover: #1d4ed8;
  --accent-active: #1e40af;
  --accent-bg: #eff4ff;
  --accent-border: #c7d6fe;

  --success: #047857;
  --success-bg: #ecfdf5;
  --success-border: #a7f3d0;

  --warning: #b45309;
  --warning-bg: #fffbeb;
  --warning-border: #fde68a;

  --danger: #b91c1c;
  --danger-hover: #991b1b;
  --danger-bg: #fef2f2;
  --danger-border: #fecaca;

  --info: #1d4ed8;
  --info-bg: #eff4ff;
  --info-border: #c7d6fe;

  --shadow-xs: 0 1px 2px rgba(13, 27, 62, 0.06);
  --shadow-sm: 0 1px 3px rgba(13, 27, 62, 0.08), 0 1px 2px rgba(13, 27, 62, 0.04);
  --shadow-md: 0 4px 12px rgba(13, 27, 62, 0.08), 0 1px 3px rgba(13, 27, 62, 0.04);
  --shadow-lg: 0 12px 32px rgba(13, 27, 62, 0.12), 0 2px 6px rgba(13, 27, 62, 0.05);
  --shadow-pop: 0 0 0 1px rgba(13, 27, 62, 0.04), 0 8px 24px rgba(13, 27, 62, 0.12);
  --ring: 0 0 0 3px rgba(37, 99, 235, 0.22);

  --avatar-1-bg: #eff4ff; --avatar-1-fg: #1d4ed8;
  --avatar-2-bg: #ecfdf5; --avatar-2-fg: #047857;
  --avatar-3-bg: #fffbeb; --avatar-3-fg: #b45309;
  --avatar-4-bg: #fef2f2; --avatar-4-fg: #b91c1c;
  --avatar-5-bg: #faf5ff; --avatar-5-fg: #7c3aed;
  --avatar-6-bg: #ecfeff; --avatar-6-fg: #0e7490;
}

[data-theme="dark"] {
  color-scheme: dark;
  --bg: #0b1020;
  --bg-subtle: #0f1530;
  --bg-elevated: #131a36;
  --bg-sunken: #080d1c;
  --bg-hover: #1a2245;
  --bg-active: #212a52;

  --text: #e6ebf5;
  --text-strong: #f6f8fc;
  --text-muted: #9aa6c1;
  --text-faint: #6b7796;
  --text-on-accent: #ffffff;

  --border: #1f2748;
  --border-strong: #2c3866;
  --border-focus: #60a5fa;

  --accent: #5b8def;
  --accent-hover: #7aa3f4;
  --accent-active: #93b6f7;
  --accent-bg: #182554;
  --accent-border: #2b3f7e;

  --success: #34d399;
  --success-bg: #0f2a23;
  --success-border: #1d4f3f;

  --warning: #fbbf24;
  --warning-bg: #2a1f08;
  --warning-border: #5a3f0e;

  --danger: #f87171;
  --danger-hover: #fca5a5;
  --danger-bg: #2a1212;
  --danger-border: #5a2020;

  --info: #60a5fa;
  --info-bg: #122148;
  --info-border: #1f3a7a;

  --shadow-xs: 0 1px 2px rgba(0, 0, 0, 0.35);
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.4), 0 1px 2px rgba(0, 0, 0, 0.25);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.4), 0 1px 3px rgba(0, 0, 0, 0.3);
  --shadow-lg: 0 12px 32px rgba(0, 0, 0, 0.5), 0 2px 6px rgba(0, 0, 0, 0.35);
  --shadow-pop: 0 0 0 1px rgba(255, 255, 255, 0.04), 0 8px 24px rgba(0, 0, 0, 0.45);
  --ring: 0 0 0 3px rgba(91, 141, 239, 0.32);

  --avatar-1-bg: #182554; --avatar-1-fg: #93b6f7;
  --avatar-2-bg: #0f2a23; --avatar-2-fg: #6ee7b7;
  --avatar-3-bg: #2a1f08; --avatar-3-fg: #fbbf24;
  --avatar-4-bg: #2a1212; --avatar-4-fg: #fca5a5;
  --avatar-5-bg: #1f1437; --avatar-5-fg: #c4b5fd;
  --avatar-6-bg: #0c2530; --avatar-6-fg: #67e8f9;
}

* { box-sizing: border-box; }

html, body {
  margin: 0;
  padding: 0;
  background: var(--bg);
  color: var(--text);
  font-family: "Geist", ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  font-size: 14px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

a { color: var(--accent); text-decoration: none; }
a:hover { color: var(--accent-hover); }
button, input, select, textarea { font: inherit; color: inherit; }
button { cursor: pointer; }
::selection { background: var(--accent-bg); color: var(--text-strong); }

::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border-strong); border-radius: 6px; border: 2px solid var(--bg); }
::-webkit-scrollbar-thumb:hover { background: var(--text-faint); }
"#
}

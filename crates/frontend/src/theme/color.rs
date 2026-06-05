//! Color tokens, resolved at runtime to CSS custom properties so a single
//! `data-theme` flip on `<html>` reskins the whole app. The variable values live
//! in [`crate::theme::global_stylesheet`]; spacing/radius/typography stay baked
//! because they do not change between light and dark.

pub const BG: &str = "var(--bg)";
pub const BG_SUBTLE: &str = "var(--bg-subtle)";
pub const BG_ELEVATED: &str = "var(--bg-elevated)";
pub const BG_SUNKEN: &str = "var(--bg-sunken)";
pub const BG_HOVER: &str = "var(--bg-hover)";
pub const BG_ACTIVE: &str = "var(--bg-active)";

pub const TEXT: &str = "var(--text)";
pub const TEXT_STRONG: &str = "var(--text-strong)";
pub const TEXT_MUTED: &str = "var(--text-muted)";
pub const TEXT_FAINT: &str = "var(--text-faint)";
pub const TEXT_ON_ACCENT: &str = "var(--text-on-accent)";

pub const BORDER: &str = "var(--border)";
pub const BORDER_STRONG: &str = "var(--border-strong)";
pub const BORDER_FOCUS: &str = "var(--border-focus)";

pub const ACCENT: &str = "var(--accent)";
pub const ACCENT_HOVER: &str = "var(--accent-hover)";
pub const ACCENT_ACTIVE: &str = "var(--accent-active)";
pub const ACCENT_BG: &str = "var(--accent-bg)";
pub const ACCENT_BORDER: &str = "var(--accent-border)";

pub const SUCCESS: &str = "var(--success)";
pub const SUCCESS_BG: &str = "var(--success-bg)";
pub const SUCCESS_BORDER: &str = "var(--success-border)";

pub const WARNING: &str = "var(--warning)";
pub const WARNING_BG: &str = "var(--warning-bg)";
pub const WARNING_BORDER: &str = "var(--warning-border)";

pub const DANGER: &str = "var(--danger)";
pub const DANGER_HOVER: &str = "var(--danger-hover)";
pub const DANGER_BG: &str = "var(--danger-bg)";
pub const DANGER_BORDER: &str = "var(--danger-border)";

pub const INFO: &str = "var(--info)";
pub const INFO_BG: &str = "var(--info-bg)";
pub const INFO_BORDER: &str = "var(--info-border)";

/// `(background, foreground)` pairs indexed by avatar tone. Each resolves to a
/// theme-aware CSS variable pair defined in [`crate::theme::global_stylesheet`].
pub const AVATAR_TONES: [(&str, &str); 6] = [
    ("var(--avatar-1-bg)", "var(--avatar-1-fg)"),
    ("var(--avatar-2-bg)", "var(--avatar-2-fg)"),
    ("var(--avatar-3-bg)", "var(--avatar-3-fg)"),
    ("var(--avatar-4-bg)", "var(--avatar-4-fg)"),
    ("var(--avatar-5-bg)", "var(--avatar-5-fg)"),
    ("var(--avatar-6-bg)", "var(--avatar-6-fg)"),
];

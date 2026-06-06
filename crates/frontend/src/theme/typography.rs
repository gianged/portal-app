pub const FONT_SANS: &str =
    "'Geist', ui-sans-serif, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
pub const FONT_MONO: &str =
    "'Geist Mono', 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace";

pub const TEXT_H1: &str = "28px";
pub const TEXT_H2: &str = "20px";
pub const TEXT_H3: &str = "16px";
pub const TEXT_BODY: &str = "14px";
pub const TEXT_SMALL: &str = "13.5px";
pub const TEXT_CAPTION: &str = "12.5px";
pub const TEXT_EYEBROW: &str = "11.5px";

pub const WEIGHT_REGULAR: &str = "400";
pub const WEIGHT_MEDIUM: &str = "500";
pub const WEIGHT_SEMIBOLD: &str = "600";
#[allow(dead_code)] // TODO: unused, I will see it
pub const WEIGHT_BOLD: &str = "700";

// Shadows and the focus ring resolve to CSS variables: the reference darkens them
// under `[data-theme="dark"]`, so they must flip with the theme like the colors do.
pub const SHADOW_XS: &str = "var(--shadow-xs)";
#[allow(dead_code)] // TODO: unused, I will see it
pub const SHADOW_SM: &str = "var(--shadow-sm)";
pub const SHADOW_MD: &str = "var(--shadow-md)";
pub const SHADOW_LG: &str = "var(--shadow-lg)";
#[allow(dead_code)] // TODO: unused, I will see it
pub const SHADOW_POP: &str = "var(--shadow-pop)";
pub const RING: &str = "var(--ring)";

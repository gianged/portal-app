pub mod color;
pub mod radius;
pub mod space;
pub mod typography;

use std::str::FromStr;

#[must_use]
pub fn class(css: impl AsRef<str>) -> String {
    // SAFETY: CSS strings come from static templates + theme constants we control;
    // a parse failure would be a programmer error caught long before this runs.
    let sheet = stylist::ast::Sheet::from_str(css.as_ref())
        .expect("CSS template is statically valid");
    stylist::Style::new(sheet)
        .expect("CSS template is statically valid")
        .get_class_name()
        .to_string()
}

#![allow(dead_code)]

use leptos::prelude::*;

use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::cluster::Cluster;
use crate::primitives::stack::Gap;
use crate::theme::{self, color, typography};
use crate::util::format;

#[component]
pub fn UserCard(#[prop(into)] name: String, #[prop(into)] role: String) -> impl IntoView {
    let name_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let role_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));

    view! {
        <Cluster gap=Gap::Sm>
            <Avatar name=name.clone() size=AvatarSize::Md tone=format::tone_for(&name) />
            <div>
                <div class=name_cls>{name}</div>
                <div class=role_cls>{role}</div>
            </div>
        </Cluster>
    }
}

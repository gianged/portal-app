//! Group index: the org directory of group cards with a create dialog.

use leptos::{prelude::*, task};
use leptos_router::components::A;

use shared::dto::group::{CreateGroupRequest, GroupDto, GroupKind};
use shared::validation::group;

use crate::features::groups::api;
use crate::features::ui;
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::load::{self, Loadable};

#[component]
pub fn GroupsIndex() -> impl IntoView {
    let groups: Loadable<Vec<GroupDto>> = Loadable::new();
    let reload = RwSignal::new(0u32);
    let create_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        load::load(groups, api::list());
    });

    let open_create = Callback::new(move |_| create_open.set(true));
    let created = Callback::new(move |()| reload.update(|n| *n += 1));

    view! {
        <Stack gap=Gap::Lg>
            <Cluster gap=Gap::Sm justify="space-between".to_string()>
                {ui::section_heading("Groups")}
                <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_create>
                    <Icon name=IconName::Plus size=14 /> "New group"
                </Button>
            </Cluster>
            {move || match groups.get() {
                None => load::note("Loading groups…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(list)) if list.is_empty() => view! {
                    <EmptyState icon=IconName::Users title="No groups yet" description="Create the first group to get started." />
                }.into_any(),
                Some(Ok(list)) => {
                    let grid = theme::class(format!(
                        "display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: {g};",
                        g = space::D4,
                    ));
                    view! { <div class=grid>{list.iter().map(group_card).collect_view()}</div> }.into_any()
                }
            }}
            <CreateGroupDialog open=create_open on_created=created />
        </Stack>
    }
}

fn group_card(g: &GroupDto) -> AnyView {
    let href = format!("/groups/{}", g.id.0);
    let name = g.name.clone();
    let desc = g.description.clone();
    let count = g.member_count;
    let kind = g.kind;
    let link = theme::class("text-decoration: none; display: block;");
    let name_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H3,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let desc_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; margin: 0; \
         display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    view! {
        <A href=href attr:class=link>
            <Card>
                <Stack gap=Gap::Sm>
                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                        <h3 class=name_cls>{name}</h3>
                        {match kind {
                            GroupKind::It => view! { <Badge variant=BadgeVariant::Accent>"IT"</Badge> }.into_any(),
                            GroupKind::Standard => ().into_any(),
                        }}
                    </Cluster>
                    <p class=desc_cls>{desc}</p>
                    {ui::subtle(&format!("{count} members"))}
                </Stack>
            </Card>
        </A>
    }
    .into_any()
}

#[component]
fn CreateGroupDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let kind = RwSignal::new(GroupKind::Standard);
    let name_err = RwSignal::new(None::<String>);
    let desc_err = RwSignal::new(None::<String>);
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_kind = Callback::new(move |v: String| {
        kind.set(GroupKind::from_wire(&v).unwrap_or(GroupKind::Standard));
    });
    let kind_value = Signal::derive(move || kind.get().as_str().to_owned());

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        name_err.set(None);
        desc_err.set(None);
        let n = name.get_untracked();
        let d = description.get_untracked();
        let mut ok = true;
        if let Err(e) = group::validate_group_name(&n) {
            name_err.set(Some(e.to_string()));
            ok = false;
        }
        if let Err(e) = group::validate_group_description(&d) {
            desc_err.set(Some(e.to_string()));
            ok = false;
        }
        if !ok {
            return;
        }
        submitting.set(true);
        let req = CreateGroupRequest {
            name: n,
            description: d,
            kind: kind.get_untracked(),
        };
        task::spawn_local(async move {
            match api::create(&req).await {
                Ok(created) => {
                    if created.authz_pending {
                        toast.success("Group created; permissions are still syncing");
                    } else {
                        toast.success("Group created");
                    }
                    name.set(String::new());
                    description.set(String::new());
                    open.set(false);
                    on_created.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="New group" subtitle="Create a team or the IT group." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="gr-name">"Name"</FieldLabel>
                        <Input value=name on_input=Callback::new(move |v| name.set(v)) placeholder="e.g. Platform Engineering" />
                        {move || name_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="gr-desc">"Description"</FieldLabel>
                        <Textarea value=description on_input=Callback::new(move |v| description.set(v)) placeholder="What does this group own?" />
                        {move || desc_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="gr-kind">"Kind"</FieldLabel>
                        <Select value=kind_value on_change=on_kind>
                            {GroupKind::ALL.into_iter().map(|k| view! {
                                <option value=k.as_str()>{k.label()}</option>
                            }).collect_view()}
                        </Select>
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting>
                    {move || if submitting.get() { "Creating…" } else { "Create group" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

//! The people directory: a searchable index of users with an HR create-user
//! dialog.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use shared::dto::user::{CreateUserRequest, SystemRole, UserDto};
use shared::validation::user::validate_create_user;

use crate::features::ui::section_heading;
use crate::features::users::api;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::{FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::table::{Table, TableToolbar, TableWrap};
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::debounce::debounced;
use crate::util::format::tone_for;
use crate::util::load::{Loadable, load, load_error, note};

fn system_role_wire(r: Option<SystemRole>) -> &'static str {
    match r {
        Some(SystemRole::Director) => "director",
        Some(SystemRole::Hr) => "hr",
        None => "",
    }
}

fn system_role_from_wire(s: &str) -> Option<SystemRole> {
    match s {
        "director" => Some(SystemRole::Director),
        "hr" => Some(SystemRole::Hr),
        _ => None,
    }
}

#[component]
pub fn UsersIndex() -> impl IntoView {
    let users: Loadable<Vec<UserDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let create_open = RwSignal::new(false);
    let search = RwSignal::new(String::new());
    let dq = debounced(search.into(), 300);

    Effect::new(move |_| {
        let _ = reload.get();
        let term = dq.get().trim().to_owned();
        load(users, api::list((!term.is_empty()).then_some(term)));
    });

    let open_create = Callback::new(move |_| create_open.set(true));
    let created = Callback::new(move |()| reload.update(|n| *n += 1));
    let search_wrap = class("width: 220px;");

    view! {
        <Stack gap=Gap::Lg>
            <TableWrap>
                <TableToolbar>
                    {section_heading("People")}
                    <Cluster gap=Gap::Sm>
                        <div class=search_wrap>
                            <Input value=search on_input=Callback::new(move |v| search.set(v)) placeholder="Search people…" />
                        </div>
                        <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_create>
                            <Icon name=IconName::Plus size=14 /> " New user"
                        </Button>
                    </Cluster>
                </TableToolbar>
                {move || match users.get() {
                    None => note("Loading people…"),
                    Some(Err(e)) => load_error(&e),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <EmptyState icon=IconName::Building title="No people yet" description="Provision the first account." />
                    }.into_any(),
                    Some(Ok(list)) => users_table(list),
                }}
            </TableWrap>
            <CreateUserDialog open=create_open on_created=created />
        </Stack>
    }
}

fn users_table(items: Vec<UserDto>) -> AnyView {
    view! {
        <Table>
            <thead>
                <tr>
                    <th>"Name"</th>
                    <th>"Email"</th>
                    <th>"Role"</th>
                    <th>"Group"</th>
                </tr>
            </thead>
            <tbody>{items.into_iter().map(user_row).collect_view()}</tbody>
        </Table>
    }
    .into_any()
}

fn user_row(u: UserDto) -> impl IntoView {
    let href = format!("/users/{}", u.id.0);
    let name = u.name.clone();
    let email = u.email.clone();
    let role = u.role.label();
    let group = u.group_name.unwrap_or_default();
    let link_cls = class(format!(
        "color: {c}; font-weight: {fw}; text-decoration: none; &:hover {{ color: {a}; }}",
        c = color::TEXT_STRONG,
        fw = typography::WEIGHT_MEDIUM,
        a = color::ACCENT,
    ));
    let wrap = class(format!(
        "display: inline-flex; align-items: center; gap: {g};",
        g = space::D2
    ));
    view! {
        <tr>
            <td>
                <span class=wrap>
                    <Avatar name=name.clone() size=AvatarSize::Sm tone=tone_for(&name) />
                    <A href=href attr:class=link_cls>{name}</A>
                </span>
            </td>
            <td><span class="cell-muted">{email}</span></td>
            <td>{role}</td>
            <td><span class="cell-muted">{group}</span></td>
        </tr>
    }
}

#[component]
fn CreateUserDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let full_name = RwSignal::new(String::new());
    let phone = RwSignal::new(String::new());
    let timezone = RwSignal::new("UTC".to_owned());
    let system_role = RwSignal::new(None::<SystemRole>);
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_role = Callback::new(move |v: String| system_role.set(system_role_from_wire(&v)));
    let role_value = Signal::derive(move || system_role_wire(system_role.get()).to_owned());

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let phone_val = phone.get_untracked();
        let req = CreateUserRequest {
            email: email.get_untracked(),
            password: password.get_untracked(),
            full_name: full_name.get_untracked(),
            phone: (!phone_val.is_empty()).then_some(phone_val),
            timezone: timezone.get_untracked(),
            system_role: system_role.get_untracked(),
        };
        if let Err(e) = validate_create_user(&req) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        spawn_local(async move {
            match api::create(&req).await {
                Ok(_) => {
                    toast.success("User created");
                    email.set(String::new());
                    password.set(String::new());
                    full_name.set(String::new());
                    phone.set(String::new());
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
            <DialogHeader title="New user" subtitle="Provision a company account." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="u-name">"Full name"</FieldLabel>
                        <Input value=full_name on_input=Callback::new(move |v| full_name.set(v)) placeholder="Jane Doe" />
                    </div>
                    <div>
                        <FieldLabel for_id="u-email">"Email"</FieldLabel>
                        <Input value=email on_input=Callback::new(move |v| email.set(v)) type_="email" placeholder="jane@company.com" />
                    </div>
                    <div>
                        <FieldLabel for_id="u-pass">"Temporary password"</FieldLabel>
                        <Input value=password on_input=Callback::new(move |v| password.set(v)) type_="password" placeholder="••••••••" />
                    </div>
                    <div>
                        <FieldLabel for_id="u-phone">"Phone (optional)"</FieldLabel>
                        <Input value=phone on_input=Callback::new(move |v| phone.set(v)) placeholder="+1 555 0100" />
                    </div>
                    <div>
                        <FieldLabel for_id="u-tz">"Timezone"</FieldLabel>
                        <Input value=timezone on_input=Callback::new(move |v| timezone.set(v)) placeholder="UTC" />
                    </div>
                    <div>
                        <FieldLabel for_id="u-role">"Org role"</FieldLabel>
                        <Select value=role_value on_change=on_role>
                            <option value="">"None"</option>
                            <option value="director">"Director"</option>
                            <option value="hr">"HR"</option>
                        </Select>
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Creating…" } else { "Create user" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

//! User UI: the reusable [`UserPicker`], the people directory with a create-user
//! dialog (HR), and the profile detail with edit + deactivate/reactivate.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use uuid::Uuid;

use shared::dto::ids::UserId;
use shared::dto::user::{
    ChangePasswordRequest, CreateUserRequest, ResetPasswordRequest, SystemRole,
    UpdateProfileRequest, UserDto, UserProfileDto, UserRole, UserStatus,
};
use shared::validation::user::{
    validate_change_password, validate_create_user, validate_reset_password,
    validate_update_profile,
};

use crate::features::auth::api as auth_api;
use crate::features::ui::{back_link, page_title, section_heading, subtle};
use crate::features::users::api;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::{FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::table::{Table, TableToolbar, TableWrap};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::debounce::debounced;
use crate::util::format::tone_for;
use crate::util::load::{Loadable, load, load_error, note};

fn status_variant(s: UserStatus) -> BadgeVariant {
    match s {
        UserStatus::Active => BadgeVariant::Success,
        UserStatus::Pending => BadgeVariant::Warning,
        UserStatus::Deactivated => BadgeVariant::Neutral,
    }
}

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

// ─────────────────────────── UserPicker (shared) ───────────────────────────

/// A `<select>` of active users, used wherever a person must be chosen (assign a
/// request/ticket, add a group member, start a DM). Loads the directory once and
/// yields the chosen [`UserId`] via `on_select`.
#[component]
pub fn UserPicker(
    #[prop(into)] selected: Signal<Option<UserId>>,
    on_select: Callback<UserId>,
    #[prop(optional, into)] placeholder: Option<String>,
) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_else(|| "Select a person…".to_owned());
    let users: Loadable<Vec<UserDto>> = RwSignal::new(None);
    load(users, api::list(None));

    let value = Signal::derive(move || selected.get().map(|u| u.0.to_string()).unwrap_or_default());
    let handle = Callback::new(move |s: String| {
        if let Ok(uuid) = Uuid::parse_str(&s) {
            on_select.run(UserId(uuid));
        }
    });

    view! {
        <Select value=value on_change=handle>
            <option value="">{placeholder}</option>
            {move || {
                users.get().and_then(Result::ok).map(|list| {
                    list.into_iter()
                        .map(|u| {
                            let id = u.id.0.to_string();
                            view! { <option value=id>{u.name}</option> }
                        })
                        .collect_view()
                })
            }}
        </Select>
    }
}

// ─────────────────────────── Directory ───────────────────────────

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

// ─────────────────────────── Detail / profile ───────────────────────────

#[component]
pub fn UserDetail(#[prop(into)] id: Signal<Option<UserId>>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let auth = use_context::<AuthState>().expect("AuthState context");
    let profile: Loadable<UserProfileDto> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let edit_open = RwSignal::new(false);
    let pwd_open = RwSignal::new(false);
    let reset_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(uid) = id.get() {
            load(profile, api::get(uid));
        }
    });

    let set_active = move |activate: bool| {
        let Some(uid) = id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            let result = if activate {
                api::reactivate(uid).await.map(|_| ())
            } else {
                api::deactivate(uid).await
            };
            match result {
                Ok(()) => {
                    toast.success(if activate {
                        "User reactivated"
                    } else {
                        "User deactivated"
                    });
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let open_edit = Callback::new(move |_| edit_open.set(true));
    let open_pwd = Callback::new(move |_| pwd_open.set(true));
    let open_reset = Callback::new(move |_| reset_open.set(true));
    let saved = Callback::new(move |()| reload.update(|n| *n += 1));

    view! {
        <Stack gap=Gap::Lg>
            {back_link("/users", "Back to people")}
            {move || match profile.get() {
                None => note("Loading profile…"),
                Some(Err(e)) => load_error(&e),
                Some(Ok(p)) => {
                    let name = p.full_name.clone();
                    let status = p.status;
                    let ep_name = p.full_name.clone();
                    let ep_phone = p.phone.clone().unwrap_or_default();
                    let ep_tz = p.timezone.clone();
                    let title_v = page_title(&p.full_name);
                    let email_v = subtle(&p.email);
                    let fields_v = profile_fields(&p);
                    let deactivate_cb = Callback::new(move |_| set_active(false));
                    let reactivate_cb = Callback::new(move |_| set_active(true));
                    let action = match status {
                        UserStatus::Deactivated => view! {
                            <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=reactivate_cb>"Reactivate"</Button>
                        }.into_any(),
                        _ => view! {
                            <Button variant=ButtonVariant::Destructive size=ButtonSize::Sm on_click=deactivate_cb>"Deactivate"</Button>
                        }.into_any(),
                    };
                    let viewer = auth.user.get();
                    let is_self = viewer.as_ref().map(|u| u.id) == id.get();
                    let is_hr = viewer.as_ref().is_some_and(|u| u.role == UserRole::Hr);
                    let pwd_action = is_self.then(|| view! {
                        <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_pwd>"Change password"</Button>
                    });
                    let reset_action = (is_hr && !is_self).then(|| view! {
                        <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_reset>"Reset password"</Button>
                    });
                    view! {
                        <Stack gap=Gap::Lg>
                            <Card>
                                <Stack gap=Gap::Md>
                                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                                        <Cluster gap=Gap::Sm>
                                            <Avatar name=name.clone() size=AvatarSize::Lg tone=tone_for(&name) />
                                            <Stack gap=Gap::Xs>
                                                {title_v}
                                                {email_v}
                                            </Stack>
                                        </Cluster>
                                        <Cluster gap=Gap::Xs>
                                            <Badge variant=status_variant(status)>{status.label()}</Badge>
                                            <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_edit>
                                                <Icon name=IconName::Edit size=14 /> " Edit"
                                            </Button>
                                            {pwd_action}
                                            {reset_action}
                                            {action}
                                        </Cluster>
                                    </Cluster>
                                    {fields_v}
                                </Stack>
                            </Card>
                            <EditProfileDialog
                                open=edit_open
                                id=id
                                full_name=ep_name
                                phone=ep_phone
                                timezone=ep_tz
                                on_saved=saved
                            />
                        </Stack>
                    }.into_any()
                }
            }}
            <ChangePasswordDialog open=pwd_open />
            <ResetPasswordDialog open=reset_open id=id />
        </Stack>
    }
}

/// Self-service password change; revokes every session server-side, so it signs the user out and lets the guard redirect.
#[component]
fn ChangePasswordDialog(open: RwSignal<bool>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let auth = use_context::<AuthState>().expect("AuthState context");
    let current = RwSignal::new(String::new());
    let new_pw = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        if new_pw.get_untracked() != confirm.get_untracked() {
            toast.error("New passwords do not match");
            return;
        }
        let req = ChangePasswordRequest {
            current_password: current.get_untracked(),
            new_password: new_pw.get_untracked(),
        };
        if let Err(e) = validate_change_password(&req) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        spawn_local(async move {
            match auth_api::change_password(&req).await {
                Ok(()) => {
                    toast.success("Password changed — sign in with the new password");
                    open.set(false);
                    // The old token is already revoked; logout just clears the
                    // stale cookie, then the auth guard redirects to /login.
                    let _ = auth_api::logout().await;
                    auth.clear();
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Change password" subtitle="You will be signed out everywhere and must log in again." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="cp-current">"Current password"</FieldLabel>
                        <Input value=current on_input=Callback::new(move |v| current.set(v)) type_="password" placeholder="••••••••" />
                    </div>
                    <div>
                        <FieldLabel for_id="cp-new">"New password"</FieldLabel>
                        <Input value=new_pw on_input=Callback::new(move |v| new_pw.set(v)) type_="password" placeholder="••••••••" />
                    </div>
                    <div>
                        <FieldLabel for_id="cp-confirm">"Confirm new password"</FieldLabel>
                        <Input value=confirm on_input=Callback::new(move |v| confirm.set(v)) type_="password" placeholder="••••••••" />
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Changing…" } else { "Change password" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

/// HR sets a temporary password (mirrors `CreateUserDialog`); the target's sessions are revoked server-side.
#[component]
fn ResetPasswordDialog(
    open: RwSignal<bool>,
    #[prop(into)] id: Signal<Option<UserId>>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let password = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let Some(uid) = id.get_untracked() else {
            return;
        };
        let req = ResetPasswordRequest {
            new_password: password.get_untracked(),
        };
        if let Err(e) = validate_reset_password(&req) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        spawn_local(async move {
            match api::reset_password(uid, &req).await {
                Ok(()) => {
                    toast.success("Temporary password set — share it securely");
                    password.set(String::new());
                    open.set(false);
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Reset password" subtitle="Set a temporary password and share it with the user out-of-band. Their sessions end immediately." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="rp-pass">"Temporary password"</FieldLabel>
                        <Input value=password on_input=Callback::new(move |v| password.set(v)) type_="password" placeholder="••••••••" />
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Resetting…" } else { "Reset password" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

fn profile_fields(p: &UserProfileDto) -> AnyView {
    let phone = p.phone.clone().unwrap_or_else(|| "—".to_owned());
    let tz = p.timezone.clone();
    let role = p
        .system_role
        .map_or_else(|| "Member".to_owned(), |r| r.label().to_owned());
    let field = |label: &str, value: String| {
        let l = class(format!(
            "font-family: {ff}; font-size: {fs}; font-weight: {fw}; text-transform: uppercase; \
             letter-spacing: 0.06em; color: {c};",
            ff = typography::FONT_SANS,
            fs = typography::TEXT_EYEBROW,
            fw = typography::WEIGHT_SEMIBOLD,
            c = color::TEXT_FAINT,
        ));
        let v = class(format!(
            "font-family: {ff}; font-size: {fs}; color: {c};",
            ff = typography::FONT_SANS,
            fs = typography::TEXT_SMALL,
            c = color::TEXT,
        ));
        let label = label.to_owned();
        view! { <Stack gap=Gap::Xs><div class=l>{label}</div><div class=v>{value}</div></Stack> }
    };
    let grid = class(format!(
        "display: grid; grid-template-columns: repeat(3, 1fr); gap: {g};",
        g = space::D5,
    ));
    view! {
        <div class=grid>
            {field("Org role", role)}
            {field("Phone", phone)}
            {field("Timezone", tz)}
        </div>
    }
    .into_any()
}

#[component]
fn EditProfileDialog(
    open: RwSignal<bool>,
    #[prop(into)] id: Signal<Option<UserId>>,
    #[prop(into)] full_name: String,
    #[prop(into)] phone: String,
    #[prop(into)] timezone: String,
    on_saved: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let name = RwSignal::new(full_name);
    let phone = RwSignal::new(phone);
    let tz = RwSignal::new(timezone);
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let Some(uid) = id.get_untracked() else {
            return;
        };
        let phone_val = phone.get_untracked();
        let req = UpdateProfileRequest {
            full_name: Some(name.get_untracked()),
            phone: (!phone_val.is_empty()).then_some(phone_val),
            timezone: Some(tz.get_untracked()),
            avatar_storage_key: None,
        };
        if let Err(e) = validate_update_profile(&req) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        spawn_local(async move {
            match api::update(uid, &req).await {
                Ok(_) => {
                    toast.success("Profile updated");
                    open.set(false);
                    on_saved.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Edit profile" subtitle="Update name, phone, and timezone." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="ep-name">"Full name"</FieldLabel>
                        <Input value=name on_input=Callback::new(move |v| name.set(v)) />
                    </div>
                    <div>
                        <FieldLabel for_id="ep-phone">"Phone"</FieldLabel>
                        <Input value=phone on_input=Callback::new(move |v| phone.set(v)) />
                    </div>
                    <div>
                        <FieldLabel for_id="ep-tz">"Timezone"</FieldLabel>
                        <Input value=tz on_input=Callback::new(move |v| tz.set(v)) />
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Saving…" } else { "Save" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

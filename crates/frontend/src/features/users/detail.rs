//! The user profile detail: identity card with status, edit, deactivate /
//! reactivate, and password change / reset dialogs.

use leptos::prelude::*;
use leptos::task::spawn_local;

use shared::dto::ids::UserId;
use shared::dto::user::{
    ChangePasswordRequest, ResetPasswordRequest, UpdateProfileRequest, UserProfileDto, UserRole,
    UserStatus,
};
use shared::validation::user::{
    validate_change_password, validate_reset_password, validate_update_profile,
};

use crate::features::auth::api as auth_api;
use crate::features::ui::{back_link, page_title, subtle};
use crate::features::users::api;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::{FieldLabel, Input};
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::format::tone_for;
use crate::util::load::{Loadable, load, load_error, note};

fn status_variant(s: UserStatus) -> BadgeVariant {
    match s {
        UserStatus::Active => BadgeVariant::Success,
        UserStatus::Pending => BadgeVariant::Warning,
        UserStatus::Deactivated => BadgeVariant::Neutral,
    }
}

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

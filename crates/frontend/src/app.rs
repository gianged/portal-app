use leptos::{prelude::*, task};
use leptos_router::{
    components::{ParentRoute, Route, Router, Routes},
    path,
};

use shared::dto::ws::ServerFrame;

use crate::api::client;
use crate::api::ws::WsClient;
use crate::features::announcements::routes::AnnouncementsPage;
use crate::features::audit::routes::AuditPage;
use crate::features::auth::{self, routes::LoginPage};
use crate::features::chat::routes::ChatPage;
use crate::features::daily_reports::routes::{DailyReportPage, TeamReportsPage};
use crate::features::day_off::routes::{LeaveApprovalsPage, TimeOffPage};
use crate::features::flex_hours::routes::{FlexApprovalsPage, FlexHoursPage};
use crate::features::groups::routes::{GroupDetailPage, GroupsPage};
use crate::features::holidays::routes::HolidaysPage;
use crate::features::home::routes::{DashboardPage, FilesPage, LandingPage, PermissionsPage};
use crate::features::home::shell::AuthedLayout;
use crate::features::leave::routes::LeavePage;
use crate::features::notifications::api;
use crate::features::notifications::routes::InboxPage;
use crate::features::overtime::routes::{OvertimeApprovalsPage, OvertimePage};
use crate::features::policy::routes::PolicyPage;
use crate::features::projects::routes::{ProjectDetailPage, ProjectsPage};
use crate::features::reports::routes::ReportsPage;
use crate::features::requests::routes::{RequestDetailPage, RequestsPage};
use crate::features::tickets::routes::{TicketDetailPage, TicketsPage};
use crate::features::users::routes::{UserDetailPage, UsersPage};
use crate::primitives::toast::ToastHost;
use crate::state;
use crate::state::auth::AuthState;
use crate::state::notifications::NotificationsState;
use crate::state::theme::ThemeState;
use crate::state::toast::ToastState;
use crate::theme;

#[component]
pub fn App() -> impl IntoView {
    let auth = AuthState::new();
    provide_context(auth);
    let notifications = NotificationsState::new();
    provide_context(notifications);
    let toast = ToastState::new();
    provide_context(toast);

    // Expired session: any 401 clears auth so `RequireAuth` redirects to /login.
    client::on_unauthorized(move || auth.clear());

    let theme = ThemeState::new();
    provide_context(theme);
    // Reflect the theme onto `<html data-theme>` + localStorage; runs on first
    // paint (seeding the stored/OS preference) and on every toggle.
    Effect::new(move |_| state::theme::apply_theme(theme.theme.get()));

    // The chat socket is created up front but only dials once the session is
    // authenticated, so a logged-out client never reconnect-loops a 401 upgrade.
    let ws = WsClient::new();
    provide_context(ws);
    // Socket-level failures (rate limit, rejected send) arrive as frames.
    let _ = ws.on_frame(move |frame| {
        if let ServerFrame::Error { message, .. } = frame {
            toast.error(message.clone());
        }
    });
    Effect::new(move |_| {
        if auth.loaded.get() && auth.is_authenticated() {
            ws.start();
        } else {
            ws.stop();
        }
    });

    // Resolve the session once on load, then mark auth as resolved so route
    // guards can act without a flash-redirect on refresh.
    task::spawn_local(async move {
        if let Ok(user) = auth::api::me().await {
            auth.set_user(user);
            if let Ok(count) = api::unread_count().await {
                notifications.set_unread(count);
            }
        }
        auth.loaded.set(true);
    });

    view! {
        <style>{theme::global_stylesheet()}</style>
        <Router>
            <Routes fallback=NotFound>
                <Route path=path!("/") view=LandingPage />
                <Route path=path!("/login") view=LoginPage />
                // Pathless layout: the shell mounts once and pages swap inside
                // its outlet, so sidebar scroll survives navigation.
                <ParentRoute path=path!("") view=AuthedLayout>
                    <Route path=path!("/dashboard") view=DashboardPage />
                    <Route path=path!("/inbox") view=InboxPage />
                    <Route path=path!("/announcements") view=AnnouncementsPage />
                    <Route path=path!("/groups") view=GroupsPage />
                    <Route path=path!("/groups/:id") view=GroupDetailPage />
                    <Route path=path!("/projects") view=ProjectsPage />
                    <Route path=path!("/projects/:id") view=ProjectDetailPage />
                    <Route path=path!("/requests") view=RequestsPage />
                    <Route path=path!("/requests/:id") view=RequestDetailPage />
                    <Route path=path!("/tickets") view=TicketsPage />
                    <Route path=path!("/tickets/:id") view=TicketDetailPage />
                    <Route path=path!("/chat") view=ChatPage />
                    <Route path=path!("/users") view=UsersPage />
                    <Route path=path!("/users/:id") view=UserDetailPage />
                    <Route path=path!("/files") view=FilesPage />
                    <Route path=path!("/permissions") view=PermissionsPage />
                    <Route path=path!("/audit") view=AuditPage />
                    <Route path=path!("/reports") view=ReportsPage />
                    <Route path=path!("/policy") view=PolicyPage />
                    <Route path=path!("/daily-reports") view=DailyReportPage />
                    <Route path=path!("/daily-reports/team") view=TeamReportsPage />
                    <Route path=path!("/leave") view=LeavePage />
                    <Route path=path!("/time-off") view=TimeOffPage />
                    <Route path=path!("/leave-approvals") view=LeaveApprovalsPage />
                    <Route path=path!("/holidays") view=HolidaysPage />
                    <Route path=path!("/overtime") view=OvertimePage />
                    <Route path=path!("/overtime-approvals") view=OvertimeApprovalsPage />
                    <Route path=path!("/flex-hours") view=FlexHoursPage />
                    <Route path=path!("/flex-approvals") view=FlexApprovalsPage />
                </ParentRoute>
            </Routes>
        </Router>
        <ToastHost />
    }
}

#[component]
fn NotFound() -> impl IntoView {
    let cls = theme::class("padding: 48px; text-align: center;");
    view! {
        <div class=cls>
            <h1>"404"</h1>
            <p>"That page doesn't exist."</p>
            <a href="/">"Go home"</a>
        </div>
    }
}

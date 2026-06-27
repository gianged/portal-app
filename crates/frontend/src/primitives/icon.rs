//! The app's Lucide-style icon set. One `Icon` component renders a 24x24 stroked
//! SVG selected by [`IconName`], tinted via `currentColor` so icons reskin with the theme.

use leptos::prelude::*;

use crate::theme::class;

#[allow(dead_code)] // TODO: unused
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Search,
    Bell,
    Sun,
    Moon,
    Plus,
    ChevronDown,
    ChevronRight,
    ChevronLeft,
    ChevronUpDown,
    More,
    Check,
    Close,
    Filter,
    Users,
    Folder,
    Inbox,
    Ticket,
    Chat,
    Megaphone,
    Settings,
    Logout,
    AlertCircle,
    AlertTriangle,
    Info,
    Send,
    Paperclip,
    Pin,
    Calendar,
    Clock,
    ArrowUp,
    ArrowDown,
    ArrowRight,
    Crown,
    Shield,
    Building,
    Lock,
    Spark,
    Doc,
    Trash,
    Edit,
}

impl IconName {
    fn inner(self) -> AnyView {
        match self {
            Self::Search => {
                view! { <circle cx="11" cy="11" r="7"></circle><path d="m20 20-3.5-3.5"></path> }
                    .into_any()
            }
            Self::Bell => view! {
                <path d="M6 8a6 6 0 1 1 12 0c0 7 3 7 3 9H3c0-2 3-2 3-9z"></path>
                <path d="M10 21a2 2 0 0 0 4 0"></path>
            }
            .into_any(),
            Self::Sun => view! {
                <circle cx="12" cy="12" r="4"></circle>
                <path d="M12 3v2M12 19v2M5 5l1.5 1.5M17.5 17.5 19 19M3 12h2M19 12h2M5 19l1.5-1.5M17.5 6.5 19 5"></path>
            }
            .into_any(),
            Self::Moon => {
                view! { <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"></path> }.into_any()
            }
            Self::Plus => view! { <path d="M12 5v14M5 12h14"></path> }.into_any(),
            Self::ChevronDown => view! { <path d="m6 9 6 6 6-6"></path> }.into_any(),
            Self::ChevronRight => view! { <path d="m9 6 6 6-6 6"></path> }.into_any(),
            Self::ChevronLeft => view! { <path d="m15 6-6 6 6 6"></path> }.into_any(),
            Self::ChevronUpDown => view! { <path d="m7 15 5 5 5-5M7 9l5-5 5 5"></path> }.into_any(),
            Self::More => view! {
                <circle cx="5" cy="12" r="1.2" fill="currentColor"></circle>
                <circle cx="12" cy="12" r="1.2" fill="currentColor"></circle>
                <circle cx="19" cy="12" r="1.2" fill="currentColor"></circle>
            }
            .into_any(),
            Self::Check => view! { <path d="M4 12.5 9.5 18 20 6"></path> }.into_any(),
            Self::Close => view! { <path d="M6 6l12 12M18 6 6 18"></path> }.into_any(),
            Self::Filter => view! { <path d="M3 5h18l-7 9v6l-4-2v-4z"></path> }.into_any(),
            Self::Users => view! {
                <circle cx="9" cy="8" r="3.5"></circle>
                <path d="M2 20c0-3.5 3-6 7-6s7 2.5 7 6"></path>
                <circle cx="17" cy="9" r="2.8"></circle>
                <path d="M22 20c0-2.7-2.2-5-5-5"></path>
            }
            .into_any(),
            Self::Folder => {
                view! { <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path> }
                    .into_any()
            }
            Self::Inbox => {
                view! { <path d="M3 13h5l1 3h6l1-3h5M3 13l3-7h12l3 7M3 13v6a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-6"></path> }
                    .into_any()
            }
            Self::Ticket => {
                view! { <path d="M3 8a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v2a2 2 0 0 0 0 4v2a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-2a2 2 0 0 0 0-4z"></path> }
                    .into_any()
            }
            Self::Chat => {
                view! { <path d="M4 5h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1h-9l-5 4v-4H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z"></path> }
                    .into_any()
            }
            Self::Megaphone => {
                view! { <path d="M3 11v2a1 1 0 0 0 1 1h2l8 4V6L6 10H4a1 1 0 0 0-1 1zM18 8a5 5 0 0 1 0 8"></path> }
                    .into_any()
            }
            Self::Settings => view! {
                <circle cx="12" cy="12" r="3"></circle>
                <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h.1A1.7 1.7 0 0 0 10 3.1V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"></path>
            }
            .into_any(),
            Self::Logout => {
                view! { <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9"></path> }
                    .into_any()
            }
            Self::AlertCircle => view! {
                <circle cx="12" cy="12" r="9"></circle>
                <path d="M12 8v5M12 16.5v.5"></path>
            }
            .into_any(),
            Self::AlertTriangle => view! {
                <path d="M10.3 4 2 18a1.5 1.5 0 0 0 1.3 2.3h17.4A1.5 1.5 0 0 0 22 18L13.7 4a1.5 1.5 0 0 0-2.6 0z"></path>
                <path d="M12 9v5M12 17.5v.5"></path>
            }
            .into_any(),
            Self::Info => view! {
                <circle cx="12" cy="12" r="9"></circle>
                <path d="M12 16v-5M12 8v.5"></path>
            }
            .into_any(),
            Self::Send => view! { <path d="M22 2 11 13M22 2l-7 20-4-9-9-4z"></path> }.into_any(),
            Self::Paperclip => {
                view! { <path d="M21 11.5 12 20.5a5.5 5.5 0 0 1-7.8-7.8l10-10a3.7 3.7 0 0 1 5.2 5.2l-10 10a1.8 1.8 0 0 1-2.6-2.6l8.5-8.5"></path> }
                    .into_any()
            }
            Self::Pin => view! { <path d="M12 17v5M9 3h6l-1 4 3 4H7l3-4z"></path> }.into_any(),
            Self::Calendar => view! {
                <rect x="3" y="5" width="18" height="16" rx="2"></rect>
                <path d="M3 10h18M8 3v4M16 3v4"></path>
            }
            .into_any(),
            Self::Clock => view! {
                <circle cx="12" cy="12" r="9"></circle>
                <path d="M12 7v5l3 2"></path>
            }
            .into_any(),
            Self::ArrowUp => view! { <path d="M12 19V5M5 12l7-7 7 7"></path> }.into_any(),
            Self::ArrowDown => view! { <path d="M12 5v14M5 12l7 7 7-7"></path> }.into_any(),
            Self::ArrowRight => view! { <path d="M5 12h14M12 5l7 7-7 7"></path> }.into_any(),
            Self::Crown => view! { <path d="M3 19h18l-2-11-5 4-3-7-3 7-5-4z"></path> }.into_any(),
            Self::Shield => {
                view! { <path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z"></path> }.into_any()
            }
            Self::Building => view! {
                <rect x="4" y="3" width="16" height="18" rx="1"></rect>
                <path d="M8 7h2M14 7h2M8 11h2M14 11h2M8 15h2M14 15h2M10 21v-3h4v3"></path>
            }
            .into_any(),
            Self::Lock => view! {
                <rect x="4" y="11" width="16" height="10" rx="2"></rect>
                <path d="M8 11V8a4 4 0 0 1 8 0v3"></path>
            }
            .into_any(),
            Self::Spark => {
                view! { <path d="M12 3v5M12 16v5M3 12h5M16 12h5M5.6 5.6l3.5 3.5M14.9 14.9l3.5 3.5M5.6 18.4l3.5-3.5M14.9 9.1l3.5-3.5"></path> }
                    .into_any()
            }
            Self::Doc => {
                view! { <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9zM14 3v6h6M8 13h8M8 17h5"></path> }
                    .into_any()
            }
            Self::Trash => {
                view! { <path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13"></path> }
                    .into_any()
            }
            Self::Edit => {
                view! { <path d="M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"></path> }
                    .into_any()
            }
        }
    }
}

/// A themed icon. `size` defaults to 16px; the glyph inherits the surrounding
/// `color` via `currentColor`, so place it inside a colored element to tint it.
#[component]
pub fn Icon(name: IconName, #[prop(optional)] size: Option<u32>) -> impl IntoView {
    let s = size.unwrap_or(16);
    let cls = class("flex-shrink: 0; display: inline-block; vertical-align: middle;");
    view! {
        <svg
            class=cls
            width=s
            height=s
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            {name.inner()}
        </svg>
    }
}

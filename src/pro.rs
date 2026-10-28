//! The PublishKit Pro window: what Pro is, the licence on this computer, and
//! where to paste a licence number.

use desktop_kit::ui::{ProBadge, ProLook};
use dioxus::prelude::*;

use crate::licence::{self, FreeError, Status};

/// Shared by every part of the UI that can open the window or needs to know
/// whether the app is licensed. Provided once, by `App`.
#[derive(Clone, Copy)]
pub struct Pro {
    pub open: Signal<bool>,
    pub status: Signal<Status>,
}

/// Sends `true` for each Escape pressed in the page. The handler is kept on
/// `window` and replaced on each mount, so two never stack.
const ESCAPE_JS: &str =
    "if (window._proEscape) window.removeEventListener('keydown', window._proEscape);\
     window._proEscape = (e) => { if (e.key === 'Escape') dioxus.send(true); };\
     window.addEventListener('keydown', window._proEscape);";

/// Opens `url` in the default browser.
fn open_url(url: &str) {
    use std::process::Command;
    let _ = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else if cfg!(windows) {
        Command::new("cmd").args(["/c", "start", "", url]).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
}

/// "Get Pro…" while free, "✦ PRO" once licensed; opens the window either way.
#[component]
pub fn ProButton() -> Element {
    let mut pro = use_context::<Pro>();
    let status = pro.status.read().clone();
    let title = match &status {
        Status::Pro(a) => format!("PublishKit Pro, {} plan for {}", a.plan.name, a.email),
        Status::NotPro(a) if a.is_free() => format!(
            "Free: {} AI credits left this month. Pro is the licence for commercial use",
            a.credits.left
        ),
        Status::NotPro(_) | Status::Offline(_) | Status::Unknown(_) => {
            "Your licence doesn't make this PublishKit Pro right now".to_owned()
        }
        Status::Free => {
            "Pro is the licence for commercial use, with AI credits each month".to_owned()
        }
    };
    rsx! {
        ProBadge { look: ProLook::of(&status), title, onclick: move |_| pro.open.set(true) }
    }
}

#[component]
pub fn ProWindow() -> Element {
    let mut pro = use_context::<Pro>();
    let mut key = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut busy = use_signal(|| false);
    let mut free_email = use_signal(String::new);
    let mut free_note = use_signal(|| Option::<String>::None);
    let mut free_hidden = use_signal(|| false);
    // Set by an Activate that made PublishKit Pro, so the window
    // congratulates rather than just reporting it.
    let mut activated = use_signal(|| false);
    // Escape closes the window wherever the focus is: the Pro view has no
    // field to listen on.
    use_future(move || async move {
        let mut open = pro.open;
        let mut keys = document::eval(ESCAPE_JS);
        while keys.recv::<bool>().await.is_ok() {
            if *open.peek() {
                open.set(false);
                activated.set(false);
            }
        }
    });
    if !(pro.open)() {
        return rsx! {};
    }
    let status = pro.status.read().clone();
    let asks_number =
        matches!(status, Status::Free | Status::Unknown(_)) || status.free_credits().is_some();
    let offers_free = matches!(status, Status::Free | Status::Unknown(_)) && !free_hidden();
    let mut close = move || {
        pro.open.set(false);
        activated.set(false);
        error.set(None);
    };
    let mut activate = move || {
        let pasted = key.read().clone();
        busy.set(true);
        spawn(async move {
            match licence::activate(&pasted).await {
                Ok(s) => {
                    activated.set(matches!(s, Status::Pro(_)));
                    pro.status.set(s);
                    key.set(String::new());
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let mut check_again = move || {
        busy.set(true);
        spawn(async move {
            pro.status.set(licence::refresh(true).await);
            busy.set(false);
        });
    };
    let mut ask_free = move || {
        let email = free_email.read().clone();
        busy.set(true);
        spawn(async move {
            match licence::request_free_credits(&email).await {
                Ok(message) => {
                    free_note.set(Some(format!(
                        "{message} Paste the licence number above when it arrives."
                    )));
                    error.set(None);
                }
                Err(FreeError::Unavailable) => free_hidden.set(true),
                Err(FreeError::Refused(e)) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let mut manage = move |number: String| {
        busy.set(true);
        spawn(async move {
            match licence::portal_url(&number).await {
                Ok(url) => {
                    open_url(&url);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    if let Status::Pro(a) = &status {
        let number = a.licence.clone();
        return rsx! {
            div { class: "modal-backdrop", onclick: move |_| close() }
            div { class: "modal pro-done",
                button { class: "modal-close", title: "Close", onclick: move |_| close(), "×" }
                div { class: "pro-done-mark", "✓" }
                h2 { class: "pro-done-title",
                    if activated() { "Congratulations, you're Pro!" } else { "You're Pro" }
                }
                p { class: "pro-done-sub", "Licence validated." }
                p { class: "pro-done-meta",
                    "{a.credits.left} AI credits left this month"
                    br {}
                    "{a.renewal()}"
                }
                button {
                    class: "pro-done-manage",
                    disabled: busy(),
                    title: "Change or cancel the plan on mayorana.ch",
                    onclick: move |_| manage(number.clone()),
                    "Manage subscription"
                }
                if let Some(e) = error.read().as_ref() {
                    p { class: "pro-done-error", "{e}" }
                }
            }
        };
    }

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| close() }
        div { class: "modal",
            div { class: "modal-head",
                h2 { "PublishKit Pro" }
                button { class: "modal-close", title: "Close", onclick: move |_| close(), "×" }
            }
            div { class: "modal-body pro-body",
                match &status {
                    Status::Free => rsx! {
                        p {
                            "PublishKit works without a licence for personal and non-commercial use. "
                            "Pro is the licence for commercial use, with AI credits each month for api0. "
                            "It comes with a monthly plan on mayorana.ch."
                        }
                    },
                    Status::Pro(_) => rsx! {},
                    Status::NotPro(a) if a.is_free() => rsx! {
                        p {
                            "Free — "
                            strong { "{a.credits.left} AI credits" }
                            " left this month, for "
                            strong { "{a.email}" }
                            ". {a.renewal()}."
                        }
                        p {
                            "Pro is the licence for commercial use, with more AI credits each month. "
                            "It comes with a monthly plan on mayorana.ch."
                        }
                    },
                    Status::NotPro(a) if !a.active => rsx! {
                        p {
                            "The subscription for "
                            strong { "{a.email}" }
                            " isn't active, so PublishKit isn't Pro. Renew it to have Pro again."
                        }
                    },
                    Status::NotPro(a) => rsx! {
                        p {
                            "The "
                            strong { "{a.plan.name}" }
                            " plan doesn't include PublishKit. A plan with PublishKit makes it Pro."
                        }
                    },
                    Status::Offline(a) => rsx! {
                        p {
                            "PublishKit hasn't been able to check the licence for "
                            strong { "{a.email}" }
                            " for a week, so it isn't Pro until it can. Check the connection, then check again."
                        }
                    },
                    Status::Unknown(_) => rsx! {
                        p { "The licence number on this computer isn't known. Check it against the email, or paste it again." }
                    },
                }
                button {
                    class: "btn-primary pro-buy",
                    onclick: move |_| open_url(licence::BUY_URL),
                    if asks_number { "Buy PublishKit Pro" } else { "Renew PublishKit Pro" }
                }
                if asks_number {
                    p { class: "settings-hint", "Already subscribed? Paste the licence number from your email:" }
                    input {
                        class: "licence-key",
                        placeholder: "ABCDE-FGHJK-LMNPQ-RSTUV",
                        value: "{key}",
                        oninput: move |e| key.set(e.value()),
                    }
                    p { class: "settings-hint",
                        "The number is checked with api0 now and once a day. Pro keeps working for a week without a connection."
                    }
                }
                if offers_free {
                    div { class: "free-credits",
                        p { class: "settings-hint", "No plan? Get free AI credits every month, without Pro. The licence number comes by email." }
                        div { class: "free-row",
                            input {
                                class: "licence-key",
                                r#type: "email",
                                placeholder: "you@example.com",
                                value: "{free_email}",
                                oninput: move |e| free_email.set(e.value()),
                            }
                            button {
                                class: "btn-small",
                                disabled: free_email.read().trim().is_empty() || busy(),
                                onclick: move |_| ask_free(),
                                "Get free AI credits"
                            }
                        }
                        if let Some(n) = free_note.read().as_ref() {
                            p { class: "pro-ok settings-hint", "{n}" }
                        }
                    }
                }
                if let Some(e) = error.read().as_ref() {
                    p { class: "pro-error", "{e}" }
                }
                div { class: "modal-actions",
                    if asks_number {
                        button {
                            class: "btn-small",
                            disabled: key.read().trim().is_empty() || busy(),
                            onclick: move |_| activate(),
                            if busy() { span { class: "pro-spinner", title: "Checking the licence" } } else { "Activate" }
                        }
                    }
                    if let (Some(number), false) = (status.licence().map(str::to_owned), asks_number) {
                        button {
                            class: "btn-small",
                            disabled: busy(),
                            title: "Change or cancel the plan on mayorana.ch",
                            onclick: move |_| manage(number.clone()),
                            "Manage subscription"
                        }
                        button {
                            class: "btn-small",
                            disabled: busy(),
                            onclick: move |_| check_again(),
                            "Check again"
                        }
                    }
                    if status.licence().is_some() {
                        button {
                            class: "btn-small",
                            title: "Removes it from every Mayorana app on this computer, to move it to another one",
                            onclick: move |_| {
                                licence::deactivate();
                                pro.status.set(licence::current());
                            },
                            "Remove the licence from this computer"
                        }
                    }
                    span { class: "pro-actions-gap" }
                    button { class: "btn-small", onclick: move |_| close(), "Close" }
                }
            }
        }
    }
}

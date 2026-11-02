//! The window that shows how to use PublishKit from Claude.

use super::*;

#[component]
pub fn McpModal(show: Signal<bool>) -> Element {
    rsx! {
        desktop_kit::ui::ConnectWindow {
            lead: "Ask Claude to write, edit and translate your drafts and your LinkedIn, X, Instagram and \
                   TikTok posts, and publish when you say so. It works on the same posts as this window, and \
                   the lists here follow.",
            try_it: "\u{201c}Write next week's post about remote interviews, in French and English, and queue \
                     both.\u{201d}",
            on_close: move |_| show.set(false),
        }
    }
}

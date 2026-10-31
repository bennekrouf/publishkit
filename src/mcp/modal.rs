//! The window that shows how to use PublishKit from Claude.

use super::*;

#[component]
pub fn McpModal(show: Signal<bool>) -> Element {
    let command = command();
    let desktop = desktop_config(&command);
    let code = code_command(&command);
    let codex = codex_command(&command);
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| show.set(false) }
        div { class: "modal mcp-modal",
            div { class: "modal-head",
                h2 { "Use PublishKit from Claude or Codex" }
                button { class: "modal-close", title: "Close", onclick: move |_| show.set(false), "×" }
            }
            div { class: "modal-body",
                p { class: "settings-hint mcp-lead",
                    "Ask Claude to write, edit and translate your drafts and your LinkedIn, X, Instagram and TikTok posts, and \
                     publish when you say so. It works on the same posts as this window, and the lists here follow."
                }
                div { class: "settings-field",
                    label { "1 · Claude Desktop" }
                    p { class: "settings-hint",
                        "Settings → Developer → Edit Config, paste this in and save, then quit Claude and open it again. \
                         If the file already has an mcpServers section, add just the publishkit entry to it."
                    }
                    Snippet { text: desktop.clone() }
                    p { class: "settings-hint mcp-path", "The file: {desktop_config_path()}" }
                }
                div { class: "settings-field",
                    label { "2 · Claude Code" }
                    p { class: "settings-hint", "Run this once in a terminal:" }
                    Snippet { text: code.clone() }
                }
                div { class: "settings-field",
                    label { "3 · Codex" }
                    p { class: "settings-hint", "Run this once in a terminal:" }
                    Snippet { text: codex.clone() }
                }
                div { class: "settings-field",
                    label { "4 · Try it" }
                    p { class: "settings-hint",
                        "Ask: “Write next week's post about remote interviews, in French and English, and queue both.”"
                    }
                }
                p { class: "settings-hint mcp-note",
                    "Claude in the browser (claude.ai) can’t use this: it only connects to servers on the internet, and \
                     PublishKit runs on your computer. Any other app that speaks MCP can, with the same command: "
                    code { "{command} mcp" }
                }
            }
        }
    }
}

/// A block of text to paste somewhere else, with its own Copy button.
#[component]
pub(super) fn Snippet(text: String) -> Element {
    let mut copied = use_signal(|| false);
    let to_copy = text.clone();
    rsx! {
        div { class: "mcp-snippet-wrap",
            pre { class: "mcp-snippet", "{text}" }
            button {
                class: "btn-small mcp-copy",
                onclick: move |_| {
                    crate::screens::copy_to_clipboard(&to_copy);
                    copied.set(true);
                },
                if copied() { "Copied ✓" } else { "Copy" }
            }
        }
    }
}

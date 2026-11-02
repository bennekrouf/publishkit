//! The form for a new post on a network's tab.

use super::*;

/// The form for a new post: written by hand, or drafted by the model and then changed.
#[component]
pub(in crate::screens) fn SocialCenterPanel(
    channel: Channel,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<SocialPost>>,
    compose: Signal<Option<Compose>>,
) -> Element {
    let mut body = use_signal(String::new);
    let mut topic = use_signal(String::new);
    let mut angle = use_signal(String::new);
    let mut link = use_signal(String::new);
    let mut lang = use_signal(String::new);
    let mut with_ai = use_signal(|| false);
    let generating = use_signal(|| false);
    let mut status_msg = use_signal(String::new);
    let languages = posts.read().languages.clone();
    let label = channel.label();

    // A post begun elsewhere, from a blog post's Share button, fills the form.
    use_effect(move || {
        let Some(begun) = compose.read().clone() else {
            return;
        };
        if begun.channel != channel {
            return;
        }
        body.set(begun.body);
        topic.set(begun.topic);
        angle.set(begun.angle);
        link.set(begun.link);
        lang.set(begun.lang);
        status_msg.set(String::new());
        let mut compose = compose;
        compose.set(None);
    });

    let placeholder = match channel {
        Channel::LinkedIn => {
            "What you want to tell your network: the news, why it matters, a link…"
        }
        Channel::X => "Your post: one idea, short, a link if it has one…",
        Channel::Instagram => {
            "The caption under your photo or video: a first line that stops the scroll, a few hashtags…"
        }
        Channel::TikTok => "The caption under your video: a hook, one idea, a few hashtags…",
    };

    rsx! {
        div { class: "panel panel-center",
            div { class: "mode-tabs",
                button { class: "tab active", "+ New {label} post" }
            }
            div { class: "form",
                div { class: "field",
                    div { class: "field-head",
                        label { "Post" }
                        CharCount { channel, text: body.read().clone() }
                    }
                    textarea {
                        class: "social-text",
                        placeholder,
                        value: "{body}",
                        oninput: move |e| body.set(e.value()),
                        rows: if channel == Channel::X { "6" } else { "12" },
                    }
                }
                if with_ai() {
                    {ai_draft(channel, config, posts, AiDraft { body, topic, angle, link, lang, with_ai, generating, status_msg })}
                } else {
                    button {
                        class: "btn-link",
                        title: "Have the model write a first version from a topic and a few points",
                        onclick: move |_| with_ai.set(true),
                        "✨ Draft with AI"
                    }
                }
                if languages.len() > 1 {
                    div { class: "field field-row",
                        label { "Language" }
                        LangTabs { languages: languages.clone(), active: lang }
                    }
                }
                if !status_msg.read().is_empty() {
                    p { class: "status-msg", "{status_msg}" }
                }
                button {
                    class: "btn-primary btn-generate",
                    disabled: generating() || body.read().trim().is_empty(),
                    onclick: move |_| {
                        let text = body.read().trim().to_string();
                        let t = match topic.read().trim() {
                            "" => api::topic_of(&text),
                            t => t.to_string(),
                        };
                        let l = shown_lang(&lang.read(), &posts.read().languages);
                        let project = config.read().project_path.clone().unwrap_or_default();
                        let pcfg = ProjectConfig::load(&project);
                        match api::save_social_to_queue(&project, &pcfg, channel, &t, &l, &text) {
                            Ok(path) => {
                                let all = load_posts(&project);
                                let saved = all
                                    .social(channel, &l)
                                    .queue
                                    .into_iter()
                                    .find(|p| p.path == std::path::Path::new(&path));
                                posts.set(all);
                                selected.set(saved);
                                body.set(String::new());
                                topic.set(String::new());
                                angle.set(String::new());
                                link.set(String::new());
                                with_ai.set(false);
                                status_msg.set(format!(
                                    "Saved to the queue: {}",
                                    path.split(['/', '\\']).next_back().unwrap_or("")
                                ));
                            }
                            Err(e) => status_msg.set(format!("Save error: {e}")),
                        }
                    },
                    "Save draft"
                }
            }
        }
    }
}

/// The form's fields, which drafting with the model reads and fills.
#[derive(Clone, Copy)]
struct AiDraft {
    body: Signal<String>,
    topic: Signal<String>,
    angle: Signal<String>,
    link: Signal<String>,
    lang: Signal<String>,
    with_ai: Signal<bool>,
    generating: Signal<bool>,
    status_msg: Signal<String>,
}

/// Drafting the post with the model, from a topic, a link and a few points.
fn ai_draft(
    channel: Channel,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    f: AiDraft,
) -> Element {
    let AiDraft {
        mut body,
        mut topic,
        mut angle,
        mut link,
        lang,
        mut with_ai,
        mut generating,
        mut status_msg,
    } = f;
    let label = channel.label();
    rsx! {
        div { class: "ai-box",
            div { class: "ai-box-head",
                span { "✨ Draft with AI" }
                button { class: "btn-micro", onclick: move |_| with_ai.set(false), "Close" }
            }
            div { class: "field",
                label { "Topic" }
                input {
                    placeholder: "e.g. The new export feature",
                    value: "{topic}",
                    oninput: move |e| topic.set(e.value()),
                }
            }
            div { class: "field",
                label { "Link (optional)" }
                input {
                    r#type: "url",
                    placeholder: "https://…",
                    title: "Put in the post as it is, on a line of its own",
                    value: "{link}",
                    oninput: move |e| link.set(e.value()),
                }
            }
            div { class: "field",
                label { "Angle / key points" }
                textarea {
                    placeholder: "What problem it solves, standout feature, who it's for…",
                    value: "{angle}",
                    oninput: move |e| angle.set(e.value()),
                    rows: "3",
                }
            }
            button {
                class: "btn-small",
                disabled: generating() || topic.read().trim().is_empty(),
                title: "The text goes in the box above, for you to change before saving",
                onclick: move |_| {
                    let t = topic.read().trim().to_string();
                    let a = angle.read().clone();
                    let u = link.read().trim().to_string();
                    let l = shown_lang(&lang.read(), &posts.read().languages);
                    let cfg = config.read().clone();
                    let project = cfg.project_path.clone().unwrap_or_default();
                    let api_key = cfg.resolved_api_key();
                    if api_key.is_empty() {
                        status_msg.set(cfg.missing_key());
                        return;
                    }
                    spawn(async move {
                        generating.set(true);
                        status_msg.set(format!("Writing the {label} post…"));
                        let pcfg = ProjectConfig::load(&project);
                        match api::generate_social_post(channel, &t, &a, &u, &l, &cfg.provider, &api_key, &pcfg).await {
                            Ok(text) => {
                                body.set(text);
                                status_msg.set("Written: change what you like, then save it.".to_string());
                            }
                            Err(e) => status_msg.set(format!("Error: {e}")),
                        }
                        generating.set(false);
                    });
                },
                if generating() {
                    "Writing…"
                } else if body.read().trim().is_empty() {
                    "Write it"
                } else {
                    "Write it again (replaces the text)"
                }
            }
        }
    }
}

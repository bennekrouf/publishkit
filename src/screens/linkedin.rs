//! The LinkedIn tab: its posts, the one being edited and its preview.

use super::*;

#[component]
pub(super) fn LinkedInList(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    li_selected: Signal<Option<LinkedInPost>>,
) -> Element {
    let active_lang = use_signal(String::new);

    let languages = posts.read().languages.clone();
    let lang = shown_lang(&active_lang.read(), &languages);
    let shelf = posts.read().linkedin(&lang);

    rsx! {
        div { class: "panel panel-left",
            LangTabs { languages, active: active_lang }

            div { class: "section-label", "Queue ({shelf.queue.len()})" }
            for post in shelf.queue {
                LinkedInItem {
                    is_selected: li_selected.read().as_ref().is_some_and(|s| s.path == post.path),
                    post,
                    config, posts, li_selected,
                    is_queue: true,
                }
            }

            div { class: "section-label section-label-published", "Posted ({shelf.published.len()})" }
            for post in shelf.published {
                LinkedInItem {
                    is_selected: li_selected.read().as_ref().is_some_and(|s| s.path == post.path),
                    post,
                    config, posts, li_selected,
                    is_queue: false,
                }
            }
        }
    }
}

#[component]
pub(super) fn LinkedInItem(
    post: LinkedInPost,
    is_selected: bool,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    li_selected: Signal<Option<LinkedInPost>>,
    is_queue: bool,
) -> Element {
    let post_click = post.clone();
    let post_pub = post.clone();
    let post_del = post.clone();
    // "Mark as posted…" open, the date and time it went out, and why it failed.
    let mut marking = use_signal(|| false);
    let mut posted_at = use_signal(String::new);
    let mut mark_error = use_signal(String::new);

    rsx! {
        div {
            class: if is_selected { "post-item selected" } else { "post-item" },
            onclick: move |_| *li_selected.write() = Some(post_click.clone()),

            div { class: "post-item-title", "{post.topic}" }
            div { class: "post-item-date", "{date_label(&post)}" }
            if let Some(at) = &post.publish_at {
                div { class: "post-item-date", title: "Planned on the calendar", "⏱ {when_label(at)}" }
            }

            if is_queue && marking() {
                div { class: "post-item-actions mark-posted", onclick: move |e| e.stop_propagation(),
                    input {
                        r#type: "datetime-local",
                        title: "When it went out on LinkedIn",
                        value: "{posted_at}",
                        oninput: move |e| posted_at.set(e.value()),
                    }
                    button {
                        class: "btn-micro btn-publish",
                        disabled: crate::plan::When::parse(&posted_at.read()).is_none(),
                        onclick: move |_| {
                            let project = config.read().project_path.clone().unwrap_or_default();
                            let pcfg = ProjectConfig::load(&project);
                            let at = crate::plan::When::parse(&posted_at.read());
                            match api::publish_linkedin_post(&project, &pcfg, &post_pub.lang, &post_pub.filename, at) {
                                Ok(()) => {
                                    marking.set(false);
                                    *posts.write() = load_posts(&project);
                                }
                                Err(e) => mark_error.set(format!("{e:#}")),
                            }
                        },
                        "Save"
                    }
                    button { class: "btn-micro", onclick: move |_| marking.set(false), "Cancel" }
                    if !mark_error.read().is_empty() {
                        div { class: "status-msg", "{mark_error}" }
                    }
                }
            } else if is_queue {
                div { class: "post-item-actions",
                    button {
                        class: "btn-micro btn-publish",
                        title: "Record it as posted, on the date and time it went out on LinkedIn",
                        onclick: move |e| {
                            e.stop_propagation();
                            posted_at.set(crate::plan::format_when(chrono::Local::now().naive_local()));
                            mark_error.set(String::new());
                            marking.set(true);
                        },
                        "Mark as posted…"
                    }
                    button {
                        class: "btn-micro btn-delete",
                        onclick: move |e| {
                            e.stop_propagation();
                            let project = config.read().project_path.clone().unwrap_or_default();
                            let pcfg = ProjectConfig::load(&project);
                            if api::delete_linkedin_queued(&project, &pcfg, &post_del.lang, &post_del.filename).is_ok() {
                                *posts.write() = load_posts(&project);
                            }
                        },
                        "✕"
                    }
                }
            }
        }
    }
}

/// A post's date as the lists show it: its day, and its time when known.
fn date_label(post: &LinkedInPost) -> String {
    match post.when {
        Some(w) => match w.time_label() {
            Some(t) => format!("{} {t}", w.day.format("%Y-%m-%d")),
            None => w.day.format("%Y-%m-%d").to_string(),
        },
        None => post.date.clone(),
    }
}

#[component]
pub(super) fn LinkedInCenterPanel(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
) -> Element {
    let mut topic = use_signal(String::new);
    let mut angle = use_signal(String::new);
    let lang = use_signal(String::new);
    let mut generating = use_signal(|| false);
    let mut status_msg = use_signal(String::new);
    let languages = posts.read().languages.clone();

    rsx! {
        div { class: "panel panel-center",
            div { class: "mode-tabs",
                button { class: "tab active", "+ New LinkedIn post" }
            }
            div { class: "form",
                div { class: "field",
                    label { "Product / topic" }
                    input {
                        placeholder: "e.g. The new export feature",
                        value: "{topic}",
                        oninput: move |e| *topic.write() = e.value(),
                    }
                }
                div { class: "field",
                    label { "Angle / key points" }
                    textarea {
                        placeholder: "What problem it solves, standout feature, who it's for...",
                        value: "{angle}",
                        oninput: move |e| *angle.write() = e.value(),
                        rows: "4",
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
                    disabled: *generating.read() || topic.read().is_empty() || angle.read().is_empty(),
                    onclick: move |_| {
                        let t = topic.read().clone();
                        let a = angle.read().clone();
                        let l = shown_lang(&lang.read(), &posts.read().languages);
                        let cfg = config.read().clone();
                        let project = cfg.project_path.clone().unwrap_or_default();
                        let provider = cfg.provider.clone();
                        let api_key = cfg.resolved_api_key();

                        if api_key.is_empty() {
                            *status_msg.write() = cfg.missing_key();
                            return;
                        }

                        spawn(async move {
                            *generating.write() = true;
                            *status_msg.write() = "Generating LinkedIn post…".to_string();
                            let pcfg = ProjectConfig::load(&project);
                            match api::generate_linkedin_post(&t, &a, &l, &provider, &api_key, &pcfg).await {
                                Ok(body) => match api::save_linkedin_to_queue(&project, &pcfg, &t, &l, &body) {
                                    Ok(path) => {
                                        *status_msg.write() = format!(
                                            "Saved: {}", path.split('/').next_back().unwrap_or("")
                                        );
                                        *posts.write() = load_posts(&project);
                                        *topic.write() = String::new();
                                        *angle.write() = String::new();
                                    }
                                    Err(e) => *status_msg.write() = format!("Save error: {e}"),
                                },
                                Err(e) => *status_msg.write() = format!("Error: {e}"),
                            }
                            *generating.write() = false;
                        });
                    },
                    if *generating.read() { "Generating…" } else { "Generate & queue" }
                }
            }
        }
    }
}

#[component]
pub(super) fn LinkedInPreviewPanel(li_selected: Signal<Option<LinkedInPost>>) -> Element {
    let post = li_selected.read().clone();
    let mut copied = use_signal(|| false);

    rsx! {
        div { class: "panel panel-right",
            if let Some(p) = post {
                div { class: "preview-meta",
                    span { class: "preview-lang-badge", "{p.lang.to_uppercase()}" }
                    span { class: "preview-date", "{date_label(&p)}" }
                    span {
                        class: if p.status == "published" { "status-badge published" } else { "status-badge draft" },
                        "{p.status}"
                    }
                }
                div { class: "social-card",
                    div { class: "social-card-header",
                        span { class: "social-icon", "in" }
                        span { "LinkedIn" }
                        button {
                            class: "btn-copy",
                            onclick: {
                                let text = p.body.clone();
                                move |_| {
                                    copy_to_clipboard(&text);
                                    *copied.write() = true;
                                }
                            },
                            if *copied.read() { "Copied ✓" } else { "Copy" }
                        }
                    }
                    p { class: "social-card-body", white_space: "pre-wrap", "{p.body}" }
                }
            } else {
                div { class: "preview-empty",
                    p { "Select a post or generate a new one to preview it here." }
                }
            }
        }
    }
}

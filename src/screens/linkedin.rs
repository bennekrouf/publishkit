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
            div { class: "post-item-date",
                "{date_label(&post)}"
                if let Some(label) = media_label(&post.media) {
                    span { class: "post-item-media", title: "Attached to the post", "{label}" }
                }
            }
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

/// What a post carries, in a word: `🖼 3`, `🎞 video`; `None` without media.
pub(crate) fn media_label(media: &[String]) -> Option<String> {
    let first = media.first()?;
    Some(match api::Media::of(first) {
        Some(api::Media::Video) => "🎞 video".to_string(),
        _ => format!("🖼 {}", media.len()),
    })
}

/// A post's media, as the preview shows them: an image as a `data:` address, a video by name.
#[derive(Clone, PartialEq)]
enum Shown {
    Image {
        name: String,
        src: String,
    },
    Video {
        name: String,
        path: std::path::PathBuf,
    },
    Missing {
        name: String,
    },
}

impl Shown {
    fn name(&self) -> &str {
        match self {
            Shown::Image { name, .. } | Shown::Video { name, .. } | Shown::Missing { name } => name,
        }
    }
}

fn shown(post: &LinkedInPost) -> Vec<Shown> {
    post.media
        .iter()
        .map(|name| {
            let path = post.media_path(name);
            let name = name.clone();
            if !path.is_file() {
                return Shown::Missing { name };
            }
            match api::Media::of(&name) {
                Some(api::Media::Video) => Shown::Video { name, path },
                _ => match local_image(&name, &post.path, "", "", "") {
                    Some(src) => Shown::Image { name, src },
                    // Too large to preview: shown by name, like a video.
                    None => Shown::Video { name, path },
                },
            }
        })
        .collect()
}

/// Opens a file in the app the system has for it.
fn open_file(path: &std::path::Path) {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    let _ = command.arg(path).spawn(); // nothing to open it with: the click does nothing
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

/// Sets a post's media to `sources`, then shows it as it is now; `error` says why not.
fn change_media(
    post: &LinkedInPost,
    sources: &[std::path::PathBuf],
    config: Signal<AppConfig>,
    mut posts: Signal<ProjectPosts>,
    mut li_selected: Signal<Option<LinkedInPost>>,
    mut error: Signal<String>,
) {
    match api::set_media(&post.path, sources) {
        Ok(_) => {
            error.set(String::new());
            let project = config.read().project_path.clone().unwrap_or_default();
            let all = load_posts(&project);
            let shelf = all.linkedin(&post.lang);
            let now = shelf
                .queue
                .iter()
                .chain(&shelf.published)
                .find(|p| p.path == post.path)
                .cloned();
            posts.set(all);
            li_selected.set(now);
        }
        Err(e) => error.set(format!("{e:#}")),
    }
}

#[component]
pub(super) fn LinkedInPreviewPanel(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    li_selected: Signal<Option<LinkedInPost>>,
) -> Element {
    let post = li_selected.read().clone();
    let mut copied = use_signal(|| false);
    let mut media_error = use_signal(String::new);
    // Read once per post: the images are encoded for the page, which a click shouldn't redo.
    let media = use_memo(move || li_selected.read().as_ref().map(shown).unwrap_or_default());
    // Another post selected: the last one's error isn't about it.
    use_effect(move || {
        li_selected.read();
        media_error.set(String::new());
    });

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
                            title: "Images or one video, copied next to the post",
                            onclick: {
                                let p = p.clone();
                                move |_| {
                                    let p = p.clone();
                                    spawn(async move {
                                        let Some(files) = rfd::AsyncFileDialog::new()
                                            .set_title("Add images or a video")
                                            .add_filter("Images and videos", &["png", "jpg", "jpeg", "gif", "mp4", "mov", "webm", "mkv", "avi"])
                                            .pick_files()
                                            .await
                                        else {
                                            return;
                                        };
                                        let mut sources: Vec<std::path::PathBuf> =
                                            p.media.iter().map(std::path::PathBuf::from).collect();
                                        sources.extend(files.iter().map(|f| f.path().to_path_buf()));
                                        change_media(&p, &sources, config, posts, li_selected, media_error);
                                    });
                                }
                            },
                            "Add media…"
                        }
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
                    if !media_error.read().is_empty() {
                        p { class: "status-msg", "{media_error}" }
                    }
                    if !media.read().is_empty() {
                        div { class: if media.read().len() == 1 { "li-media single" } else { "li-media" },
                            for item in media.read().iter().cloned() {
                                div { class: "li-media-cell",
                                    match item.clone() {
                                        Shown::Image { name, src } => rsx! {
                                            img { class: "li-media-item", src: "{src}", alt: "{name}", title: "{name}" }
                                        },
                                        Shown::Video { name, path } => rsx! {
                                            button {
                                                class: "li-media-item li-media-file",
                                                title: "Open {name}",
                                                onclick: move |_| open_file(&path),
                                                span { class: "li-media-play", "▶" }
                                                span { "{name}" }
                                            }
                                        },
                                        Shown::Missing { name } => rsx! {
                                            div { class: "li-media-item li-media-file missing",
                                                title: "Listed in the post's media, but not beside it",
                                                span { "{name}" }
                                                span { "missing" }
                                            }
                                        },
                                    }
                                    button {
                                        class: "li-media-remove",
                                        title: "Take it off the post (its copy next to the post is deleted)",
                                        onclick: {
                                            let p = p.clone();
                                            let name = item.name().to_string();
                                            move |_| {
                                                let sources: Vec<std::path::PathBuf> = p
                                                    .media
                                                    .iter()
                                                    .filter(|n| **n != name)
                                                    .map(std::path::PathBuf::from)
                                                    .collect();
                                                change_media(&p, &sources, config, posts, li_selected, media_error);
                                            }
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "preview-empty",
                    p { "Select a post or generate a new one to preview it here." }
                }
            }
        }
    }
}

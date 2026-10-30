//! The LinkedIn and X tabs: a network's posts, the one being written and its preview.

use super::*;

#[component]
pub(super) fn SocialList(
    channel: Channel,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<SocialPost>>,
) -> Element {
    let active_lang = use_signal(String::new);

    let languages = posts.read().languages.clone();
    let lang = shown_lang(&active_lang.read(), &languages);
    let shelf = posts.read().social(channel, &lang);
    let empty = shelf.queue.is_empty() && shelf.published.is_empty();

    rsx! {
        div { class: "panel panel-left",
            LangTabs { languages, active: active_lang }

            div { class: "section-label", "Queue ({shelf.queue.len()})" }
            for post in shelf.queue {
                SocialItem {
                    is_selected: selected.read().as_ref().is_some_and(|s| s.path == post.path),
                    post,
                    config, posts, selected,
                    is_queue: true,
                }
            }

            div { class: "section-label section-label-published", "Posted ({shelf.published.len()})" }
            for post in shelf.published {
                SocialItem {
                    is_selected: selected.read().as_ref().is_some_and(|s| s.path == post.path),
                    post,
                    config, posts, selected,
                    is_queue: false,
                }
            }
            if empty {
                p { class: "empty-hint", "No {channel.label()} posts yet: write one, and it waits here until you post it." }
            }
        }
    }
}

#[component]
pub(super) fn SocialItem(
    post: SocialPost,
    is_selected: bool,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<SocialPost>>,
    is_queue: bool,
) -> Element {
    let post_click = post.clone();
    let post_pub = post.clone();
    let post_del = post.clone();
    let label = post.channel.label();
    // "Mark as posted…" open, the date and time it went out, and why it failed.
    let mut marking = use_signal(|| false);
    let mut posted_at = use_signal(String::new);
    let mut mark_error = use_signal(String::new);

    rsx! {
        div {
            class: if is_selected { "post-item selected" } else { "post-item" },
            onclick: move |_| *selected.write() = Some(post_click.clone()),

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
                        title: "When it went out on {label}",
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
                            match api::publish_social_post(&project, &pcfg, post_pub.channel, &post_pub.lang, &post_pub.filename, at) {
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
                        title: "Record it as posted, on the date and time it went out on {label}",
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
                            if api::delete_social_queued(&project, &pcfg, post_del.channel, &post_del.lang, &post_del.filename).is_ok() {
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

fn shown(post: &SocialPost) -> Vec<Shown> {
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
fn date_label(post: &SocialPost) -> String {
    match post.when {
        Some(w) => match w.time_label() {
            Some(t) => format!("{} {t}", w.day.format("%Y-%m-%d")),
            None => w.day.format("%Y-%m-%d").to_string(),
        },
        None => post.date.clone(),
    }
}

/// `212 / 280`, red past the network's limit.
#[component]
fn CharCount(channel: Channel, text: String) -> Element {
    let count = text.trim().chars().count();
    let limit = channel.limit();
    let tip = match channel {
        Channel::X => "X takes 280 characters; longer posts need X Premium",
        Channel::LinkedIn => "LinkedIn takes 3,000 characters",
    };
    rsx! {
        span {
            class: if count > limit { "char-count over" } else { "char-count" },
            title: "{tip}",
            "{count} / {limit}"
        }
    }
}

/// The form for a new post: written by hand, or drafted by the model and then changed.
#[component]
pub(super) fn SocialCenterPanel(
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
    let mut generating = use_signal(|| false);
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

/// Reads the posts again and selects `path` as it is now.
fn reselect(
    path: &std::path::Path,
    channel: Channel,
    lang: &str,
    config: Signal<AppConfig>,
    mut posts: Signal<ProjectPosts>,
    mut selected: Signal<Option<SocialPost>>,
) {
    let project = config.read().project_path.clone().unwrap_or_default();
    let all = load_posts(&project);
    let shelf = all.social(channel, lang);
    let now = shelf
        .queue
        .iter()
        .chain(&shelf.published)
        .find(|p| p.path == path)
        .cloned();
    posts.set(all);
    selected.set(now);
}

/// Sets a post's media to `sources`, then shows it as it is now; `error` says why not.
fn change_media(
    post: &SocialPost,
    sources: &[std::path::PathBuf],
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<SocialPost>>,
    mut error: Signal<String>,
) {
    match api::set_media(&post.path, post.channel, sources) {
        Ok(_) => {
            error.set(String::new());
            reselect(
                &post.path,
                post.channel,
                &post.lang,
                config,
                posts,
                selected,
            );
        }
        Err(e) => error.set(format!("{e:#}")),
    }
}

#[component]
pub(super) fn SocialPreviewPanel(
    channel: Channel,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<SocialPost>>,
) -> Element {
    // The post picked on another network's tab isn't shown on this one.
    let post = selected.read().clone().filter(|p| p.channel == channel);
    let mut copied = use_signal(|| false);
    let mut media_error = use_signal(String::new);
    // The text being changed, while it is.
    let mut editing = use_signal(|| None::<String>);
    let mut share_status = use_signal(String::new);
    let mut posting = use_signal(|| false);
    // Read once per post: the images are encoded for the page, which a click shouldn't redo.
    let media = use_memo(move || selected.read().as_ref().map(shown).unwrap_or_default());
    // Another post selected: the last one's error and edit aren't about it.
    use_effect(move || {
        selected.read();
        media_error.set(String::new());
        editing.set(None);
        share_status.set(String::new());
        copied.set(false);
    });
    // Short posts go to Bluesky and Mastodon as they are, with the accounts set up in Settings.
    let accounts = config.read().crosspost.clone();
    let networks: Vec<crate::social::Network> = if channel == Channel::X {
        crate::social::Network::ALL
            .into_iter()
            .filter(|n| n.configured(&accounts))
            .collect()
    } else {
        vec![]
    };
    let icon = match channel {
        Channel::LinkedIn => "social-icon",
        Channel::X => "social-icon social-icon-x",
    };

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
                        span { class: icon, "{channel.badge()}" }
                        span { "{channel.label()}" }
                        if editing.read().is_none() {
                            button {
                                class: "btn-copy",
                                title: "Change the text",
                                onclick: {
                                    let text = p.body.trim().to_string();
                                    move |_| editing.set(Some(text.clone()))
                                },
                                "Edit"
                            }
                        }
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
                                        change_media(&p, &sources, config, posts, selected, media_error);
                                    });
                                }
                            },
                            "Add media…"
                        }
                        button {
                            class: "btn-copy",
                            onclick: {
                                let text = p.body.trim().to_string();
                                move |_| {
                                    copy_to_clipboard(&text);
                                    *copied.write() = true;
                                }
                            },
                            if *copied.read() { "Copied ✓" } else { "Copy" }
                        }
                    }
                    if let Some(text) = editing.read().clone() {
                        div { class: "social-edit",
                            textarea {
                                class: "social-text",
                                value: "{text}",
                                rows: if channel == Channel::X { "6" } else { "12" },
                                oninput: move |e| editing.set(Some(e.value())),
                            }
                            div { class: "social-edit-bar",
                                CharCount { channel, text: text.clone() }
                                button { class: "btn-micro", onclick: move |_| editing.set(None), "Cancel" }
                                button {
                                    class: "btn-micro btn-publish",
                                    disabled: text.trim().is_empty(),
                                    onclick: {
                                        let p = p.clone();
                                        move |_| {
                                            let text = editing.read().clone().unwrap_or_default();
                                            match api::set_body(&p.path, text.trim()) {
                                                Ok(()) => {
                                                    editing.set(None);
                                                    reselect(&p.path, p.channel, &p.lang, config, posts, selected);
                                                }
                                                Err(e) => media_error.set(format!("{e:#}")),
                                            }
                                        }
                                    },
                                    "Save"
                                }
                            }
                        }
                    } else {
                        // Addresses open in the browser; what is saved and copied stays the text.
                        p { class: "social-card-body", white_space: "pre-wrap",
                            for (piece, is_link) in api::split_links(p.body.trim()) {
                                if is_link {
                                    a { class: "social-link", href: "{piece}", title: "Open in your browser", "{piece}" }
                                } else {
                                    "{piece}"
                                }
                            }
                        }
                        div { class: "social-card-foot",
                            CharCount { channel, text: p.body.clone() }
                        }
                    }
                    if !media_error.read().is_empty() {
                        p { class: "status-msg", "{media_error}" }
                    }
                    if channel == Channel::LinkedIn && !p.media.is_empty() && api::split_links(&p.body).iter().any(|(_, l)| *l) {
                        p { class: "social-note",
                            "LinkedIn won't show a link preview card on posts with images or video. The link stays clickable."
                        }
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
                                                change_media(&p, &sources, config, posts, selected, media_error);
                                            }
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                    }
                    if !networks.is_empty() && editing.read().is_none() {
                        div { class: "social-share",
                            for network in networks.clone() {
                                button {
                                    class: "btn-copy",
                                    disabled: posting(),
                                    title: "Post this text on {network.label()} now",
                                    onclick: {
                                        let text = p.body.trim().to_string();
                                        let accounts = accounts.clone();
                                        move |_| {
                                            let text = text.clone();
                                            let accounts = accounts.clone();
                                            posting.set(true);
                                            share_status.set(format!("Posting on {}…", network.label()));
                                            spawn(async move {
                                                share_status.set(match crate::social::post(network, &accounts, &text).await {
                                                    Ok(url) => format!("On {}: {url}", network.label()),
                                                    Err(e) => format!("Couldn't post on {}: {e:#}", network.label()),
                                                });
                                                posting.set(false);
                                            });
                                        }
                                    },
                                    "Post on {network.label()}"
                                }
                            }
                        }
                    }
                    if !share_status.read().is_empty() {
                        p { class: "status-msg", "{share_status}" }
                    }
                }
            } else {
                div { class: "preview-empty",
                    p { "Select a post or write a new one to preview it here." }
                }
            }
        }
    }
}

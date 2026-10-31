//! The networks' tabs (LinkedIn, X, Instagram, TikTok): a network's posts, the one being written and its preview.

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
                } else if post.channel.needs_media() {
                    span {
                        class: "post-item-media missing",
                        title: "{label} needs a photo or a video: add one from its preview",
                        "no media"
                    }
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
        Some(api::Media::Video) if media.len() == 1 => "🎞 video".to_string(),
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

/// What a post is missing or will look like on its network, a line each: for the notes
/// under its preview.
pub(super) fn notes(post: &SocialPost) -> Vec<String> {
    let channel = post.channel;
    let label = channel.label();
    let has_link = api::split_links(&post.body).iter().any(|(_, l)| *l);
    let mut notes = Vec::new();
    if channel.needs_media() && post.media.is_empty() {
        notes.push(format!(
            "{label} needs a photo or a video: add one with Add media…, the text is its caption."
        ));
    }
    if has_link && !channel.links_clickable() {
        notes.push(format!(
            "Links in a {label} caption can't be clicked: put it in your bio and say so here."
        ));
    }
    if channel == Channel::LinkedIn && !post.media.is_empty() && has_link {
        notes.push(
            "LinkedIn won't show a link preview card on posts with images or video. The link stays clickable."
                .to_string(),
        );
    }
    if let Some(max) = channel.max_hashtags() {
        let tags = crate::state::hashtags(&post.body);
        if tags > max {
            notes.push(format!(
                "{label} takes at most {max} hashtags in a post or reel; this has {tags}. The \
                 rest may be ignored, or the post refused."
            ));
        }
    }
    // Size and shape: read from each image's header, never enforced. A video's isn't read.
    let mut sized = post
        .media
        .iter()
        .filter(|name| api::Media::of(name) == Some(api::Media::Image))
        .filter_map(|name| {
            let size = imagesize::size(post.media_path(name)).ok()?;
            image_note(channel, name, size.width, size.height)
        });
    if let Some(first) = sized.next() {
        let more = sized.count();
        notes.push(match more {
            0 => first,
            n => format!("{first} ({n} more like it.)"),
        });
    }
    if channel == Channel::Instagram && post.media.len() > 10 {
        notes.push(
            "Instagram's app takes 20 photos and videos in a carousel, but some accounts and \
             posting tools still stop at 10."
                .to_string(),
        );
    }
    notes
}

/// What a network will do to an image of `width`×`height`, when it isn't what the network
/// recommends: cropped to its shape, or shown soft for being small. Only a warning: the
/// networks take other sizes too.
pub(super) fn image_note(
    channel: Channel,
    name: &str,
    width: usize,
    height: usize,
) -> Option<String> {
    if width == 0 || height == 0 {
        return None;
    }
    let ratio = width as f64 / height as f64;
    let size = format!("{name} is {width}×{height}");
    match channel {
        // Instagram's feed takes 4:5 to 1.91:1 and crops the rest.
        Channel::Instagram if !(0.79..=1.92).contains(&ratio) => Some(format!(
            "{size}: Instagram crops photos to between 4:5 and 1.91:1, as the preview shows. \
             Recommended 1080×1350 (4:5)."
        )),
        Channel::Instagram if width < 1080 => Some(format!(
            "{size}: Instagram shows photos 1080 pixels wide, so it may look soft. \
             Recommended 1080×1350 (4:5)."
        )),
        // 9:16 is 0.5625: anything much wider shows with bars or cropped.
        Channel::TikTok if ratio > 0.6 => Some(format!(
            "{size}: TikTok fills the screen at 9:16, so it shows cropped or with bars. \
             Recommended 1080×1920, vertical."
        )),
        Channel::TikTok if width < 1080 => Some(format!(
            "{size}: TikTok shows it 1080 pixels wide, so it may look soft. Recommended 1080×1920."
        )),
        Channel::LinkedIn if width.max(height) < 1080 => Some(format!(
            "{size}, small for LinkedIn: 1080×1080, 1080×1350 or 1200×627 look sharp. Other \
             sizes usually work."
        )),
        Channel::X if width.max(height) < 1080 => Some(format!(
            "{size}, small for X: 1600×900 or 1080×1080 look sharp. Other sizes usually work."
        )),
        _ => None,
    }
}

/// `212 / 280`, red past the network's limit.
#[component]
fn CharCount(channel: Channel, text: String) -> Element {
    let count = text.trim().chars().count();
    let limit = channel.limit();
    let tip = match channel {
        Channel::X => "X takes 280 characters; up to 25,000 with X Premium",
        Channel::LinkedIn => "LinkedIn takes 3,000 characters",
        Channel::Instagram => "Instagram takes 2,200 characters and 5 hashtags in a caption",
        Channel::TikTok => "TikTok's app takes 4,000 characters in a caption",
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
        Channel::Instagram => "social-icon social-icon-ig",
        Channel::TikTok => "social-icon social-icon-tt",
    };
    // Instagram and TikTok show the photos or the video first, the caption under them, in
    // the shape the feed crops them to.
    let card = if channel.needs_media() {
        "social-card media-first"
    } else {
        "social-card"
    };
    let shape = match channel {
        Channel::Instagram => " media-ig",
        Channel::TikTok => " media-tt",
        _ => "",
    };
    let media_kinds = if channel.mixes_media() {
        "Photos and videos"
    } else {
        "Images or one video"
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
                if let Some(origin) = p.origin.clone() {
                    Origin { origin }
                }
                div { class: card,
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
                            title: "{media_kinds}, copied next to the post",
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
                    for note in notes(&p) {
                        p { class: "social-note", "{note}" }
                    }
                    if !media.read().is_empty() {
                        div { class: if media.read().len() == 1 { "li-media single{shape}" } else { "li-media{shape}" },
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

/// Where a repurposed draft came from: the clip to cut from the recording, or the carousel's
/// slides to make.
#[component]
fn Origin(origin: crate::repurpose::Origin) -> Element {
    use crate::repurpose::clock;
    let mut copied = use_signal(|| false);
    let slides = origin.slides.clone();
    let cut = match origin.clip {
        Some((start, end)) => Some(format!(
            "Cut {} → {} ({} s)",
            clock(start),
            clock(end),
            (end - start).round()
        )),
        None if !origin.quote.is_empty() => {
            Some(format!("Cut the passage starting “{}”", origin.quote))
        }
        None => None,
    };
    rsx! {
        div { class: "repurpose-origin",
            div { class: "repurpose-origin-head",
                span { "From " }
                button {
                    class: "btn-link",
                    title: "{origin.source}",
                    onclick: {
                        let source = origin.source.clone();
                        move |_| open_file(std::path::Path::new(&source))
                    },
                    "{origin.source_name()}"
                }
            }
            if let Some(cut) = cut {
                p { class: "repurpose-cut", "{cut}" }
            }
            if !origin.hook.is_empty() {
                p { class: "repurpose-hook", "On screen: “{origin.hook}”" }
            }
            if !slides.is_empty() {
                ol { class: "repurpose-slides",
                    for (i, slide) in slides.iter().enumerate() {
                        li { key: "{i}", "{slide}" }
                    }
                }
                button {
                    class: "btn-micro",
                    onclick: move |_| {
                        copy_to_clipboard(&slides.join("\n\n"));
                        copied.set(true);
                    },
                    if copied() { "Copied ✓" } else { "Copy the slides" }
                }
            }
        }
    }
}

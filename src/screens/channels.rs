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
    // The post's address once it is up, kept with it (optional).
    let mut posted_url = use_signal(String::new);

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
                    input {
                        r#type: "url",
                        class: "posted-url",
                        placeholder: "Its address (optional)",
                        title: "Where the post is on {label}, to open it from the library",
                        value: "{posted_url}",
                        oninput: move |e| posted_url.set(e.value()),
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
                                    let url = posted_url.read().trim().to_string();
                                    if url.starts_with("http") {
                                        let (_, posted) = pcfg.resolve_social_paths(&project, post_pub.channel, &post_pub.lang);
                                        let _ = api::set_url(&posted.join(&post_pub.filename), &url);
                                    }
                                    posted_url.set(String::new());
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

mod compose;
mod preview;
pub(super) use compose::SocialCenterPanel;
pub(super) use preview::SocialPreviewPanel;

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

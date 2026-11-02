//! A network's post as it will look, and everything done to it there: editing its text, its
//! media, copying it, posting it by hand or to Bluesky and Mastodon.

use super::*;

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

/// Everything to post a draft by hand: its text on the clipboard, the network's page open in
/// the browser, its first photo or video selected in its folder to drag in. Says what to do next.
fn post_now(post: &SocialPost) -> String {
    copy_to_clipboard(post.body.trim());
    let label = post.channel.label();
    if !desktop_kit::browser::open(post.channel.compose_url()) {
        return format!("Text copied. Open {label} in your browser and paste it.");
    }
    let first = post.media.first().map(|m| post.media_path(m));
    match first {
        Some(file) if desktop_kit::launch::reveal(&file).is_ok() => format!(
            "Text copied and {label} opened: paste the text, drag in the {} selected in its folder, post, then Mark as posted.",
            if post.media.len() > 1 { "files" } else { "file" }
        ),
        _ if post.channel.needs_media() => format!("Text copied and {label} opened, but the post has no photo or video yet."),
        _ => format!("Text copied and {label} opened: paste it, post, then Mark as posted."),
    }
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
            crate::library::attached(&post.path, post.channel, sources);
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
pub(in crate::screens) fn SocialPreviewPanel(
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
    // The library picker for the post's photos and videos, while it is open.
    let picking = use_signal(|| false);
    let posting = use_signal(|| false);
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
                        {card_actions(&p, channel, media_kinds, editing, copied, share_status, picking)}
                    }
                    {card_body(&p, channel, editing, Ctx { config, posts, selected, media_error })}
                    if !media_error.read().is_empty() {
                        p { class: "status-msg", "{media_error}" }
                    }
                    for note in notes(&p) {
                        p { class: "social-note", "{note}" }
                    }
                    if !media.read().is_empty() {
                        {media_grid(&p, media, shape, Ctx { config, posts, selected, media_error })}
                    }
                    if !networks.is_empty() && editing.read().is_none() {
                        {cross_post(&p, networks.clone(), accounts.clone(), posting, share_status)}
                    }
                    if !share_status.read().is_empty() {
                        p { class: "status-msg", "{share_status}" }
                    }
                }
                if picking() {
                    {media_picker(&p, channel, media_kinds, picking, Ctx { config, posts, selected, media_error })}
                }
            } else {
                div { class: "preview-empty",
                    p { "Select a post or write a new one to preview it here." }
                }
            }
        }
    }
}

/// What changing a post needs to show it as it is afterwards, and where its error goes.
#[derive(Clone, Copy)]
struct Ctx {
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<SocialPost>>,
    media_error: Signal<String>,
}

/// The library's photos and videos for the post: clips first where the feed is video, words
/// from the post bringing what it's about to the top. Chosen ones are added to what it has.
fn media_picker(
    p: &SocialPost,
    channel: Channel,
    media_kinds: &str,
    mut picking: Signal<bool>,
    ctx: Ctx,
) -> Element {
    use desktop_kit::library::{pick::Want, Role};
    let Ctx {
        config,
        posts,
        selected,
        media_error,
    } = ctx;
    let roles: &[Role] = if channel.needs_media() {
        &[Role::Clip, Role::Video, Role::Image, Role::Thumbnail]
    } else {
        &[Role::Image, Role::Thumbnail, Role::Clip, Role::Video]
    };
    let current: Vec<std::path::PathBuf> = p.media.iter().map(|m| p.media_path(m)).collect();
    let want = Want::new(
        "publishkit",
        format!("{media_kinds} for this {} post", channel.label()),
    )
    .roles(roles)
    .extensions(&[
        "png", "jpg", "jpeg", "gif", "webp", "mp4", "mov", "webm", "mkv", "avi",
    ])
    .multiple()
    .about(&p.body)
    .exclude(current);
    let post = p.clone();
    rsx! {
        desktop_kit::ui::FilePicker {
            want,
            on_pick: move |files: Vec<std::path::PathBuf>| {
                picking.set(false);
                let mut sources: Vec<std::path::PathBuf> = post.media.iter().map(std::path::PathBuf::from).collect();
                sources.extend(files);
                change_media(&post, &sources, config, posts, selected, media_error);
            },
            on_close: move |_| picking.set(false),
        }
    }
}

/// The card's buttons: Edit, Add media, Copy, Post now.
fn card_actions(
    p: &SocialPost,
    channel: Channel,
    media_kinds: &str,
    mut editing: Signal<Option<String>>,
    mut copied: Signal<bool>,
    mut share_status: Signal<String>,
    mut picking: Signal<bool>,
) -> Element {
    rsx! {
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
            onclick: move |_| picking.set(true),
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
        if p.status != "published" {
            button {
                class: "btn-copy",
                title: "Copies the text, opens {channel.label()} in your browser and shows the media to attach in its folder",
                onclick: {
                    let p = p.clone();
                    move |_| share_status.set(post_now(&p))
                },
                "Post now"
            }
        }
    }
}

/// The post's text, or the box it is being changed in.
fn card_body(
    p: &SocialPost,
    channel: Channel,
    mut editing: Signal<Option<String>>,
    ctx: Ctx,
) -> Element {
    let Ctx {
        config,
        posts,
        selected,
        mut media_error,
    } = ctx;
    rsx! {
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
    }
}

/// The post's photos and videos, each with a button to take it off.
fn media_grid(p: &SocialPost, media: Memo<Vec<Shown>>, shape: &str, ctx: Ctx) -> Element {
    let Ctx {
        config,
        posts,
        selected,
        media_error,
    } = ctx;
    rsx! {
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
}

/// Posting a short post's text on Bluesky and Mastodon, with the accounts set up in Settings.
fn cross_post(
    p: &SocialPost,
    networks: Vec<crate::social::Network>,
    accounts: crate::crosspost::Accounts,
    mut posting: Signal<bool>,
    mut share_status: Signal<String>,
) -> Element {
    rsx! {
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
}

#[cfg(test)]
mod tests;

//! The post being edited and its preview.

use super::*;

/// The post list's right edge, dragged to widen it; a double-click puts it back.
#[component]
pub(super) fn PanelResizer(
    resizing: Signal<bool>,
    left_width: Signal<f64>,
    config: Signal<AppConfig>,
) -> Element {
    rsx! {
        div {
            class: "panel-resizer",
            title: "Drag to widen the list — double-click to reset",
            onmousedown: move |e| {
                e.prevent_default();
                resizing.set(true);
            },
            ondoubleclick: move |_| {
                left_width.set(LEFT_PANEL_DEFAULT);
                let mut cfg = config.write();
                cfg.left_panel_width = None;
                cfg.save();
            },
        }
    }
}

#[component]
pub(super) fn CenterPanel(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
) -> Element {
    let mut title = use_signal(String::new);
    let mut summary = use_signal(String::new);
    let mut lang = use_signal(String::new);
    let languages = posts.read().languages.clone();
    let mut generating = use_signal(|| false);
    let mut status_msg = use_signal(String::new);

    // A post picked opens in the editor; the new-post form keeps only its language.
    use_effect(move || {
        if let Some(post) = selected.read().clone() {
            *lang.write() = post.lang.clone();
            *editor_mode.write() = true;
        }
    });

    rsx! {
        div { class: "panel panel-center",
            div { class: "mode-tabs",
                button {
                    class: if !*editor_mode.read() { "tab active" } else { "tab" },
                    onclick: move |_| { *editor_mode.write() = false; *selected.write() = None; },
                    "+ New post"
                }
                button {
                    class: if *editor_mode.read() { "tab active" } else { "tab" },
                    disabled: selected.read().is_none(),
                    onclick: move |_| *editor_mode.write() = true,
                    "Edit selected"
                }
            }

            if !*editor_mode.read() {
                div { class: "form",
                    div { class: "field",
                        label { "Title" }
                        input {
                            placeholder: "Why most onboarding emails go unread",
                            value: "{title}",
                            oninput: move |e| *title.write() = e.value(),
                        }
                    }
                    div { class: "field",
                        label { "Summary / angle" }
                        textarea {
                            placeholder: "The angle, the key points, who it is for…",
                            value: "{summary}",
                            oninput: move |e| *summary.write() = e.value(),
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
                        disabled: *generating.read() || title.read().is_empty() || summary.read().is_empty(),
                        onclick: move |_| {
                            let t = title.read().clone();
                            let s = summary.read().clone();
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
                                *status_msg.write() = "Generating blog post…".to_string();
                                let pcfg = ProjectConfig::load(&project);
                                match api::generate_post(&t, &s, &l, &provider, &api_key, &pcfg).await {
                                    Ok(body) => match api::save_to_queue(&project, &t, &s, &l, &body, &pcfg) {
                                        Ok(path) => {
                                            // Generate social media teasers
                                            if !pcfg.social_enabled {
                                                *status_msg.write() = format!(
                                                    "Saved: {}",
                                                    path.split('/').next_back().unwrap_or("")
                                                );
                                                *posts.write() = load_posts(&project);
                                                *generating.write() = false;
                                                return;
                                            }
                                            *status_msg.write() = "Generating social posts…".to_string();
                                            let name = std::path::Path::new(&path).file_name()
                                                .map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                            let blog_url = pcfg.post_url(&l, &name);
                                            match api::generate_social_posts(
                                                &t, &s, &l, &blog_url, &provider, &api_key
                                            ).await {
                                                Ok(social) => {
                                                    let _ = api::save_social(&path, &social);
                                                    *status_msg.write() = format!(
                                                        "Saved: {} + social posts",
                                                        path.split('/').next_back().unwrap_or("")
                                                    );
                                                }
                                                Err(e) => {
                                                    *status_msg.write() = format!(
                                                        "Saved: {} (social failed: {e})",
                                                        path.split('/').next_back().unwrap_or("")
                                                    );
                                                }
                                            }
                                            *posts.write() = load_posts(&project);
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
            } else {
                crate::editor::PostEditor { config, posts, selected }
            }
        }
    }
}

#[component]
pub(super) fn PreviewPanel(selected: Signal<Option<Post>>, config: Signal<AppConfig>) -> Element {
    let post = selected.read().clone();
    let mut copied = use_signal(String::new);
    let mut social_status = use_signal(String::new);
    let mut posting = use_signal(|| false);
    let accounts = config.read().crosspost.clone();
    let networks: Vec<crate::social::Network> = crate::social::Network::ALL
        .into_iter()
        .filter(|n| n.configured(&accounts))
        .collect();

    rsx! {
        div { class: "panel panel-right",
            if let Some(p) = post {
                div { class: "preview-meta",
                    span { class: "preview-lang-badge", "{p.lang.to_uppercase()}" }
                    span { class: "preview-date", "{p.date}" }
                    span {
                        class: if p.status == "published" { "status-badge published" } else { "status-badge draft" },
                        "{p.status}"
                    }
                }

                // Social posts section
                if let Some(social) = &p.social {
                    div { class: "social-section",
                        h3 { class: "social-heading", "Social Posts" }

                        div { class: "social-card",
                            div { class: "social-card-header",
                                span { class: "social-icon", "in" }
                                span { "LinkedIn" }
                                button {
                                    class: "btn-copy",
                                    onclick: {
                                        let text = social.linkedin.clone();
                                        move |_| {
                                            copy_to_clipboard(&text);
                                            *copied.write() = "linkedin".to_string();
                                        }
                                    },
                                    if copied.read().as_str() == "linkedin" { "Copied ✓" } else { "Copy" }
                                }
                            }
                            p { class: "social-card-body", "{social.linkedin}" }
                        }

                        div { class: "social-card",
                            div { class: "social-card-header",
                                span { class: "social-icon social-icon-x", "𝕏" }
                                span { "X / Twitter" }
                                button {
                                    class: "btn-copy",
                                    onclick: {
                                        let text = social.twitter.clone();
                                        move |_| {
                                            copy_to_clipboard(&text);
                                            *copied.write() = "twitter".to_string();
                                        }
                                    },
                                    if copied.read().as_str() == "twitter" { "Copied ✓" } else { "Copy" }
                                }
                            }
                            p { class: "social-card-body", "{social.twitter}" }
                            if !networks.is_empty() {
                                div { class: "social-share",
                                    for network in networks.clone() {
                                        button {
                                            class: "btn-copy",
                                            disabled: posting(),
                                            title: "Post this teaser on {network.label()} now",
                                            onclick: {
                                                let text = social.twitter.clone();
                                                let accounts = accounts.clone();
                                                move |_| {
                                                    let text = text.clone();
                                                    let accounts = accounts.clone();
                                                    posting.set(true);
                                                    social_status.set(format!("Posting on {}…", network.label()));
                                                    spawn(async move {
                                                        social_status.set(match crate::social::post(network, &accounts, &text).await {
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
                            if !social_status.read().is_empty() {
                                p { class: "status-msg", "{social_status}" }
                            }
                        }
                    }
                }

                div {
                    class: "preview-content blog-content",
                    dangerous_inner_html: "{preview_html(&p, &config.read().project_path.clone().unwrap_or_default())}",
                }
            } else {
                div { class: "preview-empty",
                    p { "Select a post or generate a new one to preview it here." }
                }
            }
        }
    }
}

/// The post as HTML, its own images shown: the web view can't load files from the site's
/// folder, so they are put in the page.
pub(super) fn preview_html(post: &Post, project: &str) -> String {
    let html = md_to_html(&post.full_content);
    if project.is_empty() {
        return html;
    }
    let cfg = ProjectConfig::load(project);
    let (images_dir, images_url) = cfg.images(project);
    let mut out = String::with_capacity(html.len());
    let mut rest = html.as_str();
    while let Some(i) = rest.find("<img src=\"") {
        let start = i + "<img src=\"".len();
        let Some(len) = rest[start..].find('"') else {
            break;
        };
        let src = &rest[start..start + len];
        out.push_str(&rest[..start]);
        out.push_str(
            &local_image(src, &post.path, project, &images_dir, &images_url)
                .unwrap_or_else(|| src.to_string()),
        );
        rest = &rest[start + len..];
    }
    out.push_str(rest);
    out
}

/// A site image as a `data:` address: by the site's images address, from the folder static
/// files are served from, or beside the post.
pub(super) fn local_image(
    src: &str,
    post: &std::path::Path,
    project: &str,
    images_dir: &str,
    images_url: &str,
) -> Option<String> {
    use base64::Engine;
    if src.contains("://") || src.starts_with("data:") || src.is_empty() {
        return None;
    }
    let src = src.split(['?', '#']).next()?;
    let root = std::path::Path::new(project);
    let candidates: Vec<std::path::PathBuf> = match src.strip_prefix('/') {
        Some(site_path) => {
            let mut c = vec![];
            if let Some(rest) = src.strip_prefix(images_url.trim_end_matches('/')) {
                c.push(root.join(images_dir).join(rest.trim_start_matches('/')));
            }
            c.extend(
                ["public", "static", ""]
                    .iter()
                    .map(|d| root.join(d).join(site_path)),
            );
            c
        }
        None => vec![post.parent()?.join(src)],
    };
    let file = candidates.into_iter().find(|p| p.is_file())?;
    let mime = match file.extension()?.to_str()?.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        _ => return None,
    };
    // Past a few megabytes the page would slow down; the box stays empty instead.
    if std::fs::metadata(&file).ok()?.len() > 8 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(&file).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

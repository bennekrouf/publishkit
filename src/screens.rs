use crate::api;
use crate::state::{AppConfig, LinkedInPost, LlmProvider, Post, ProjectConfig, ProjectPosts, content_stamp, load_posts};
use dioxus::prelude::*;
use pulldown_cmark::{Parser, Options, html};

#[derive(Clone, Copy, Debug, PartialEq)]
enum ContentMode { Blog, LinkedIn }

// ── Root ─────────────────────────────────────────────────────────────────────

#[component]
pub fn App() -> Element {
    let config = use_signal(|| AppConfig::load());
    let posts = use_signal(|| ProjectPosts::empty());
    let selected = use_signal(|| None::<Post>);
    let editor_mode = use_signal(|| false);
    let show_settings = use_signal(|| false);
    let content_mode = use_signal(|| ContentMode::Blog);
    let li_selected = use_signal(|| None::<LinkedInPost>);
    let mut is_light = use_signal(|| false);

    // Follow the OS light/dark setting on startup — CSS already defaults to
    // this via a `prefers-color-scheme` media query, so this just keeps the
    // toggle button's icon in sync with that initial state.
    use_hook(|| {
        spawn(async move {
            if let Ok(prefers_light) = document::eval(
                "return window.matchMedia('(prefers-color-scheme: light)').matches;"
            ).await {
                if let Some(v) = prefers_light.as_bool() {
                    is_light.set(v);
                }
            }
        });
    });

    // Anonymous usage statistics. On by default, but only once the person has
    // been told: `start` reads the opt-outs (including a shell profile's),
    // records the launch, and says whether the notice is still owed.
    let mut ask_consent = use_signal(|| false);
    use_future(move || async move {
        if crate::telemetry::start().await {
            ask_consent.set(true);
        }
        crate::telemetry::flush_forever().await;
    });
    // Recorded while the notice is on screen, so collection starts from the
    // next event — never from one the person had no chance to read about.
    use_effect(move || {
        if ask_consent() {
            crate::telemetry::mark_informed();
        }
    });

    let has_project = config.read().project_path.is_some();

    rsx! {
        style { {include_str!("../assets/main.css")} }
        // Usage-statistics notice: once, along the bottom edge. Either button
        // is remembered.
        if ask_consent() {
            div { class: "consent-banner",
                span { class: "consent-banner-text",
                    strong { "PublishKit shares anonymous usage statistics. " }
                    "Whether it is installed and opened, and its version and operating system \
                     — never your posts, files, keys or anything you type."
                }
                button {
                    class: "consent-banner-button",
                    onclick: move |_| {
                        crate::telemetry::set_consent(true);
                        ask_consent.set(false);
                    },
                    "OK"
                }
                button {
                    class: "consent-banner-button",
                    onclick: move |_| {
                        crate::telemetry::set_consent(false);
                        ask_consent.set(false);
                    },
                    "Turn off"
                }
            }
        }
        if has_project {
            MainLayout { config, posts, selected, editor_mode, show_settings, content_mode, li_selected, is_light }
        } else {
            SetupScreen { config, posts }
        }
    }
}

// ── Setup screen ──────────────────────────────────────────────────────────────

#[component]
fn SetupScreen(config: Signal<AppConfig>, posts: Signal<ProjectPosts>) -> Element {
    rsx! {
        div { class: "setup-screen",
            div { class: "setup-card",
                h1 { "Blog Manager" }
                p { class: "setup-subtitle",
                    "Select your blog project folder. You can configure the LLM provider afterwards."
                }
                p { class: "setup-hint",
                    "Expected structure: "
                    code { "content/{{fr,en}}/{{queue,blog}}/*.md" }
                }
                button {
                    class: "btn-primary",
                    onclick: move |_| {
                        spawn(async move {
                            if let Some(path) = rfd::AsyncFileDialog::new()
                                .set_title("Select blog project folder")
                                .pick_folder()
                                .await
                            {
                                let path_str = path.path().to_string_lossy().to_string();
                                *posts.write() = load_posts(&path_str);
                                let mut cfg = config.read().clone();
                                cfg.project_path = Some(path_str);
                                cfg.save();
                                *config.write() = cfg;
                            }
                        });
                    },
                    "Open Project Folder"
                }
            }
        }
    }
}

// ── Main layout ───────────────────────────────────────────────────────────────

#[component]
fn MainLayout(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
    show_settings: Signal<bool>,
    content_mode: Signal<ContentMode>,
    li_selected: Signal<Option<LinkedInPost>>,
    mut is_light: Signal<bool>,
) -> Element {
    let mut show_mcp = use_signal(|| false);
    // What Claude changes through `publishkit mcp` shows up in the lists.
    use_future(move || async move {
        let mut last: Option<Vec<(std::path::PathBuf, std::time::SystemTime)>> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let Some(project) = config.peek().project_path.clone() else { continue };
            let stamp = tokio::task::spawn_blocking({
                let project = project.clone();
                move || content_stamp(&project)
            })
            .await
            .unwrap_or_default();
            if last.as_ref().is_some_and(|l| *l != stamp) {
                let mut posts = posts;
                posts.set(load_posts(&project));
            }
            last = Some(stamp);
        }
    });
    rsx! {
        div { class: "layout",
            div { class: "topbar",
                span { class: "topbar-title", "Blog Manager" }
                span { class: "topbar-project-name", {
                    let p = config.read().project_path.clone().unwrap_or_default();
                    ProjectConfig::load(&p).site_name
                }}
                div { class: "mode-tabs",
                    button {
                        class: if *content_mode.read() == ContentMode::Blog { "tab active" } else { "tab" },
                        onclick: move |_| *content_mode.write() = ContentMode::Blog,
                        "Blog"
                    }
                    button {
                        class: if *content_mode.read() == ContentMode::LinkedIn { "tab active" } else { "tab" },
                        onclick: move |_| *content_mode.write() = ContentMode::LinkedIn,
                        "LinkedIn"
                    }
                }
                span { class: "topbar-path",
                    { config.read().project_path.as_deref().unwrap_or("") }
                }
                // provider badge
                span { class: "provider-badge",
                    { config.read().provider.label() }
                }
                button {
                    class: "btn-small",
                    onclick: move |_| {
                        let p = config.read().project_path.clone().unwrap_or_default();
                        *posts.write() = load_posts(&p);
                    },
                    "↻"
                }
                button {
                    class: "btn-small btn-danger",
                    onclick: move |_| {
                        let mut cfg = config.write();
                        cfg.project_path = None;
                        cfg.save();
                    },
                    "Change folder"
                }
                // theme toggle
                button {
                    class: "btn-theme",
                    title: if *is_light.read() { "Switch to dark mode" } else { "Switch to light mode" },
                    onclick: move |_| {
                        let next = !*is_light.read();
                        is_light.set(next);
                        document::eval(&format!("document.body.className = '{}';", if next { "light" } else { "dark" }));
                    },
                    if *is_light.read() { "🌙" } else { "☀" }
                }
                button {
                    class: "btn-small",
                    title: "Use PublishKit from Claude Desktop or Claude Code",
                    onclick: move |_| show_mcp.set(true),
                    "MCP"
                }
                // gear icon button
                button {
                    class: "btn-icon",
                    title: "Settings",
                    onclick: move |_| {
                        let v = *show_settings.read();
                        *show_settings.write() = !v;
                    },
                    "⚙"
                }
            }

            div { class: "panels",
                if *content_mode.read() == ContentMode::Blog {
                    PostList { config, posts, selected }
                    CenterPanel { config, posts, selected, editor_mode }
                    PreviewPanel { selected }
                } else {
                    LinkedInList { config, posts, li_selected }
                    LinkedInCenterPanel { config, posts }
                    LinkedInPreviewPanel { li_selected }
                }
            }

            // Settings modal overlay
            if *show_settings.read() {
                SettingsModal { config, show_settings }
            }
            if show_mcp() {
                crate::mcp::McpModal { show: show_mcp }
            }
        }
    }
}

// ── Settings modal ────────────────────────────────────────────────────────────

#[component]
fn SettingsModal(config: Signal<AppConfig>, show_settings: Signal<bool>) -> Element {
    let cfg = config.read().clone();

    let mut selected_provider = use_signal(|| cfg.provider.clone());
    let mut api_key_input = use_signal(|| cfg.api_key.clone().unwrap_or_default());
    let mut show_key = use_signal(|| false);
    let mut saved_msg = use_signal(|| String::new());

    rsx! {
        // backdrop
        div {
            class: "modal-backdrop",
            onclick: move |_| *show_settings.write() = false,
        }
        div { class: "modal",
            div { class: "modal-header",
                h2 { "Settings" }
                button {
                    class: "btn-icon",
                    onclick: move |_| *show_settings.write() = false,
                    "✕"
                }
            }

            div { class: "modal-body",
                // LLM provider
                div { class: "settings-field",
                    label { "LLM Provider" }
                    div { class: "provider-selector",
                        button {
                            class: if *selected_provider.read() == LlmProvider::DeepSeek
                                { "provider-btn active" } else { "provider-btn" },
                            onclick: move |_| *selected_provider.write() = LlmProvider::DeepSeek,
                            div { class: "provider-btn-name", "DeepSeek" }
                            div { class: "provider-btn-model", "deepseek-chat" }
                        }
                        button {
                            class: if *selected_provider.read() == LlmProvider::Claude
                                { "provider-btn active" } else { "provider-btn" },
                            onclick: move |_| *selected_provider.write() = LlmProvider::Claude,
                            div { class: "provider-btn-name", "Claude" }
                            div { class: "provider-btn-model", "claude-sonnet-4-6" }
                        }
                    }
                }

                // API key
                div { class: "settings-field",
                    label {
                        {match *selected_provider.read() {
                            LlmProvider::DeepSeek => "DeepSeek API Key",
                            LlmProvider::Claude   => "Anthropic API Key",
                        }}
                    }
                    div { class: "key-input-row",
                        input {
                            r#type: if *show_key.read() { "text" } else { "password" },
                            placeholder: match *selected_provider.read() {
                                LlmProvider::DeepSeek => "sk-...",
                                LlmProvider::Claude   => "sk-ant-...",
                            },
                            value: "{api_key_input}",
                            oninput: move |e| *api_key_input.write() = e.value(),
                        }
                        button {
                            class: "btn-icon btn-toggle-key",
                            title: if *show_key.read() { "Hide" } else { "Show" },
                            onclick: move |_| {
                                let v = *show_key.read();
                                *show_key.write() = !v;
                            },
                            if *show_key.read() { "🙈" } else { "👁" }
                        }
                    }
                    p { class: "settings-hint",
                        "Leave empty to use the "
                        code {
                            {match *selected_provider.read() {
                                LlmProvider::DeepSeek => "DEEPSEEK_API_KEY",
                                LlmProvider::Claude   => "ANTHROPIC_API_KEY",
                            }}
                        }
                        " environment variable."
                    }
                }

                if !saved_msg.read().is_empty() {
                    p { class: "settings-saved", "{saved_msg}" }
                }
            }

            div { class: "modal-footer",
                button {
                    class: "btn-primary",
                    onclick: move |_| {
                        let mut cfg = config.write();
                        cfg.provider = selected_provider.read().clone();
                        let key = api_key_input.read().trim().to_string();
                        cfg.api_key = if key.is_empty() { None } else { Some(key) };
                        cfg.save();
                        drop(cfg);
                        *saved_msg.write() = "Saved ✓".to_string();
                    },
                    "Save"
                }
            }
        }
    }
}

// ── Left panel ────────────────────────────────────────────────────────────────

#[component]
fn PostList(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
) -> Element {
    let mut active_lang = use_signal(|| "fr".to_string());

    let p = posts.read();
    let lang = active_lang.read().clone();
    let (queue, published) = if lang == "fr" {
        (p.queue_fr.clone(), p.published_fr.clone())
    } else {
        (p.queue_en.clone(), p.published_en.clone())
    };
    drop(p);

    rsx! {
        div { class: "panel panel-left",
            div { class: "lang-tabs",
                button {
                    class: if active_lang.read().as_str() == "fr" { "tab active" } else { "tab" },
                    onclick: move |_| *active_lang.write() = "fr".to_string(),
                    "FR"
                }
                button {
                    class: if active_lang.read().as_str() == "en" { "tab active" } else { "tab" },
                    onclick: move |_| *active_lang.write() = "en".to_string(),
                    "EN"
                }
            }

            div { class: "section-label", "Queue ({queue.len()})" }
            for post in queue {
                PostItem {
                    post: post.clone(),
                    is_selected: selected.read().as_ref().map(|s| s.filename == post.filename).unwrap_or(false),
                    config, posts, selected,
                    active_lang,
                    is_queue: true,
                }
            }

            div { class: "section-label section-label-published", "Published ({published.len()})" }
            for post in published {
                PostItem {
                    post: post.clone(),
                    is_selected: selected.read().as_ref().map(|s| s.filename == post.filename).unwrap_or(false),
                    config, posts, selected,
                    active_lang,
                    is_queue: false,
                }
            }
        }
    }
}

#[component]
fn PostItem(
    post: Post,
    is_selected: bool,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    active_lang: Signal<String>,
    is_queue: bool,
) -> Element {
    let post_click = post.clone();
    let post_pub   = post.clone();
    let post_del   = post.clone();
    let lang_pub   = active_lang.read().clone();
    let lang_del   = lang_pub.clone();

    rsx! {
        div {
            class: if is_selected { "post-item selected" } else { "post-item" },
            onclick: move |_| *selected.write() = Some(post_click.clone()),

            div { class: "post-item-title", "{post.title}" }
            div { class: "post-item-date",  "{post.date}" }

            if is_queue {
                div { class: "post-item-actions",
                    button {
                        class: "btn-micro btn-publish",
                        onclick: move |e| {
                            e.stop_propagation();
                            let project = config.read().project_path.clone().unwrap_or_default();
                            let pcfg = ProjectConfig::load(&project);
                            if api::publish_post(&project, &lang_pub, &post_pub.filename, &pcfg).is_ok() {
                                *posts.write() = load_posts(&project);
                            }
                        },
                        "Publish"
                    }
                    button {
                        class: "btn-micro btn-delete",
                        onclick: move |e| {
                            e.stop_propagation();
                            let project = config.read().project_path.clone().unwrap_or_default();
                            let pcfg = ProjectConfig::load(&project);
                            if api::delete_queued(&project, &lang_del, &post_del.filename, &pcfg).is_ok() {
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

// ── Center panel ──────────────────────────────────────────────────────────────

#[component]
fn CenterPanel(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
) -> Element {
    let mut title          = use_signal(|| String::new());
    let mut summary        = use_signal(|| String::new());
    let mut lang           = use_signal(|| "fr".to_string());
    let mut generating     = use_signal(|| false);
    let mut status_msg     = use_signal(|| String::new());
    let mut editor_content = use_signal(|| String::new());

    use_effect(move || {
        if let Some(post) = selected.read().clone() {
            *title.write()          = post.title.clone();
            *summary.write()        = post.description.clone();
            *lang.write()           = post.lang.clone();
            *editor_content.write() = post.full_content.clone();
            *editor_mode.write()    = true;
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
                            placeholder: "Votre carrière est-elle un carré dans un rond ?",
                            value: "{title}",
                            oninput: move |e| *title.write() = e.value(),
                        }
                    }
                    div { class: "field",
                        label { "Summary / angle" }
                        textarea {
                            placeholder: "Explorer le concept du P-E Fit...",
                            value: "{summary}",
                            oninput: move |e| *summary.write() = e.value(),
                            rows: "4",
                        }
                    }
                    div { class: "field field-row",
                        label { "Language" }
                        div { class: "lang-tabs",
                            button {
                                class: if lang.read().as_str() == "fr" { "tab active" } else { "tab" },
                                onclick: move |_| *lang.write() = "fr".to_string(),
                                "FR"
                            }
                            button {
                                class: if lang.read().as_str() == "en" { "tab active" } else { "tab" },
                                onclick: move |_| *lang.write() = "en".to_string(),
                                "EN"
                            }
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
                            let l = lang.read().clone();
                            let cfg = config.read().clone();
                            let project = cfg.project_path.clone().unwrap_or_default();
                            let provider = cfg.provider.clone();
                            let api_key = cfg.resolved_api_key();

                            if api_key.is_empty() {
                                *status_msg.write() = "No API key configured — open ⚙ Settings.".to_string();
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
                                                    path.split('/').last().unwrap_or("")
                                                );
                                                *posts.write() = load_posts(&project);
                                                *generating.write() = false;
                                                return;
                                            }
                                            *status_msg.write() = "Generating social posts…".to_string();
                                            let slug = path.split('/').last().unwrap_or("post")
                                                .replace(".md", "");
                                            let blog_url = format!(
                                                "{}/{}/blog/{}",
                                                pcfg.site_url, l, slug
                                            );
                                            match api::generate_social_posts(
                                                &t, &s, &l, &blog_url, &provider, &api_key
                                            ).await {
                                                Ok(social) => {
                                                    let _ = api::save_social(&path, &social);
                                                    *status_msg.write() = format!(
                                                        "Saved: {} + social posts",
                                                        path.split('/').last().unwrap_or("")
                                                    );
                                                }
                                                Err(e) => {
                                                    *status_msg.write() = format!(
                                                        "Saved: {} (social failed: {e})",
                                                        path.split('/').last().unwrap_or("")
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
                div { class: "editor-wrap",
                    textarea {
                        class: "md-editor",
                        value: "{editor_content}",
                        oninput: move |e| {
                            let val = e.value();
                            *editor_content.write() = val.clone();
                            let updated = selected.read().clone().map(|mut p| {
                                p.full_content = val;
                                p
                            });
                            *selected.write() = updated;
                        },
                        spellcheck: "false",
                    }
                    if !status_msg.read().is_empty() {
                        p { class: "status-msg", "{status_msg}" }
                    }
                    button {
                        class: "btn-primary",
                        onclick: move |_| {
                            if let Some(post) = selected.read().clone() {
                                let project = config.read().project_path.clone().unwrap_or_default();
                                let path = format!("{}/content/{}/queue/{}", project, post.lang, post.filename);
                                match std::fs::write(&path, editor_content.read().as_str()) {
                                    Ok(_) => {
                                        *status_msg.write() = "Saved.".to_string();
                                        *posts.write() = load_posts(&project);
                                    }
                                    Err(e) => *status_msg.write() = format!("Save error: {e}"),
                                }
                            }
                        },
                        "Save changes"
                    }
                }
            }
        }
    }
}

// ── Right panel — preview ──────────────────────────────────────────────────────

#[component]
fn PreviewPanel(selected: Signal<Option<Post>>) -> Element {
    let post = selected.read().clone();
    let mut copied = use_signal(|| String::new());

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
                        }
                    }
                }

                div {
                    class: "preview-content blog-content",
                    dangerous_inner_html: "{md_to_html(&p.full_content)}",
                }
            } else {
                div { class: "preview-empty",
                    p { "Select a post or generate a new one to preview it here." }
                }
            }
        }
    }
}

// ── LinkedIn: left panel ─────────────────────────────────────────────────────

#[component]
fn LinkedInList(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    li_selected: Signal<Option<LinkedInPost>>,
) -> Element {
    let mut active_lang = use_signal(|| "fr".to_string());

    let p = posts.read();
    let lang = active_lang.read().clone();
    let (queue, published) = if lang == "fr" {
        (p.linkedin_queue_fr.clone(), p.linkedin_published_fr.clone())
    } else {
        (p.linkedin_queue_en.clone(), p.linkedin_published_en.clone())
    };
    drop(p);

    rsx! {
        div { class: "panel panel-left",
            div { class: "lang-tabs",
                button {
                    class: if active_lang.read().as_str() == "fr" { "tab active" } else { "tab" },
                    onclick: move |_| *active_lang.write() = "fr".to_string(),
                    "FR"
                }
                button {
                    class: if active_lang.read().as_str() == "en" { "tab active" } else { "tab" },
                    onclick: move |_| *active_lang.write() = "en".to_string(),
                    "EN"
                }
            }

            div { class: "section-label", "Queue ({queue.len()})" }
            for post in queue {
                LinkedInItem {
                    post: post.clone(),
                    is_selected: li_selected.read().as_ref().map(|s| s.filename == post.filename).unwrap_or(false),
                    config, posts, li_selected,
                    active_lang,
                    is_queue: true,
                }
            }

            div { class: "section-label section-label-published", "Posted ({published.len()})" }
            for post in published {
                LinkedInItem {
                    post: post.clone(),
                    is_selected: li_selected.read().as_ref().map(|s| s.filename == post.filename).unwrap_or(false),
                    config, posts, li_selected,
                    active_lang,
                    is_queue: false,
                }
            }
        }
    }
}

#[component]
fn LinkedInItem(
    post: LinkedInPost,
    is_selected: bool,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    li_selected: Signal<Option<LinkedInPost>>,
    active_lang: Signal<String>,
    is_queue: bool,
) -> Element {
    let post_click = post.clone();
    let post_pub   = post.clone();
    let post_del   = post.clone();
    let lang_pub   = active_lang.read().clone();
    let lang_del   = lang_pub.clone();

    rsx! {
        div {
            class: if is_selected { "post-item selected" } else { "post-item" },
            onclick: move |_| *li_selected.write() = Some(post_click.clone()),

            div { class: "post-item-title", "{post.topic}" }
            div { class: "post-item-date",  "{post.date}" }

            if is_queue {
                div { class: "post-item-actions",
                    button {
                        class: "btn-micro btn-publish",
                        title: "Mark as posted (after you've published it on LinkedIn)",
                        onclick: move |e| {
                            e.stop_propagation();
                            let project = config.read().project_path.clone().unwrap_or_default();
                            if api::publish_linkedin_post(&project, &lang_pub, &post_pub.filename).is_ok() {
                                *posts.write() = load_posts(&project);
                            }
                        },
                        "Mark posted"
                    }
                    button {
                        class: "btn-micro btn-delete",
                        onclick: move |e| {
                            e.stop_propagation();
                            let project = config.read().project_path.clone().unwrap_or_default();
                            if api::delete_linkedin_queued(&project, &lang_del, &post_del.filename).is_ok() {
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

// ── LinkedIn: center panel — generate ────────────────────────────────────────

#[component]
fn LinkedInCenterPanel(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
) -> Element {
    let mut topic       = use_signal(|| String::new());
    let mut angle       = use_signal(|| String::new());
    let mut lang        = use_signal(|| "fr".to_string());
    let mut generating  = use_signal(|| false);
    let mut status_msg  = use_signal(|| String::new());

    rsx! {
        div { class: "panel panel-center",
            div { class: "mode-tabs",
                button { class: "tab active", "+ New LinkedIn post" }
            }
            div { class: "form",
                div { class: "field",
                    label { "Product / topic" }
                    input {
                        placeholder: "e.g. GitAgent — automated PR review",
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
                div { class: "field field-row",
                    label { "Language" }
                    div { class: "lang-tabs",
                        button {
                            class: if lang.read().as_str() == "fr" { "tab active" } else { "tab" },
                            onclick: move |_| *lang.write() = "fr".to_string(),
                            "FR"
                        }
                        button {
                            class: if lang.read().as_str() == "en" { "tab active" } else { "tab" },
                            onclick: move |_| *lang.write() = "en".to_string(),
                            "EN"
                        }
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
                        let l = lang.read().clone();
                        let cfg = config.read().clone();
                        let project = cfg.project_path.clone().unwrap_or_default();
                        let provider = cfg.provider.clone();
                        let api_key = cfg.resolved_api_key();

                        if api_key.is_empty() {
                            *status_msg.write() = "No API key configured — open ⚙ Settings.".to_string();
                            return;
                        }

                        spawn(async move {
                            *generating.write() = true;
                            *status_msg.write() = "Generating LinkedIn post…".to_string();
                            match api::generate_linkedin_post(&t, &a, &l, &provider, &api_key).await {
                                Ok(body) => match api::save_linkedin_to_queue(&project, &t, &l, &body) {
                                    Ok(path) => {
                                        *status_msg.write() = format!(
                                            "Saved: {}", path.split('/').last().unwrap_or("")
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

// ── LinkedIn: right panel — preview ──────────────────────────────────────────

#[component]
fn LinkedInPreviewPanel(li_selected: Signal<Option<LinkedInPost>>) -> Element {
    let post = li_selected.read().clone();
    let mut copied = use_signal(|| false);

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

pub(crate) fn copy_to_clipboard(text: &str) {
    // Dioxus desktop runs in a webview — use JS clipboard API via document eval
    let escaped = text.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${");
    let js = format!("navigator.clipboard.writeText(`{escaped}`)");
    let _ = dioxus::prelude::document::eval(&js);
}

fn md_to_html(content: &str) -> String {
    let body = if content.starts_with("---") {
        let rest = &content[3..];
        if let Some(end) = rest.find("\n---") { rest[end + 4..].trim_start_matches('\n') }
        else { content }
    } else { content };

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(body, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

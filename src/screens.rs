use crate::api;
use crate::state::{AppConfig, Drafts, LinkedInPost, LlmProvider, Post, Preset, ProjectConfig, ProjectPosts, content_stamp, load_posts, site_label};
use dioxus::prelude::*;
use pulldown_cmark::{Parser, Options, html};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ContentMode { Blog, Board, Calendar, LinkedIn }

// ── Root ─────────────────────────────────────────────────────────────────────

#[component]
pub fn App() -> Element {
    let config = use_signal(|| AppConfig::load());
    // The site open last time, read at once: the watcher only reloads after a change.
    let posts = use_signal(|| match config.peek().project_path.as_deref() {
        Some(project) => load_posts(project),
        None => ProjectPosts::empty(),
    });
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

    // PublishKit Pro: checked with api0 at start, then once a day.
    let pro = use_context_provider(|| crate::pro::Pro {
        open: Signal::new(false),
        status: Signal::new(crate::licence::current()),
    });
    // The title bar follows the licence, however it changes.
    use_effect(move || {
        dioxus::desktop::window().set_title(&crate::window_title(&pro.status.read()));
    });
    let mut pro_status = pro.status;
    use_future(move || async move {
        pro_status.set(crate::licence::refresh(true).await);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            pro_status.set(crate::licence::refresh(false).await);
        }
    });

    let has_project = config.read().project_path.is_some();

    rsx! {
        style { {include_str!("../assets/main.css")} }
        // Usage-statistics notice: once, along the bottom edge. Either button
        // is remembered.
        if ask_consent() {
            div { class: "consent-banner",
                span { class: "update-banner-text",
                    strong { "PublishKit shares anonymous usage statistics. " }
                    "Whether it is installed and opened, and its version and operating system \
                     — never your posts, files, keys or anything you type."
                }
                button {
                    class: "update-banner-link",
                    onclick: move |_| {
                        crate::telemetry::set_consent(true);
                        ask_consent.set(false);
                    },
                    "OK"
                }
                button {
                    class: "update-banner-link",
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
            SetupScreen { config, posts, selected, editor_mode }
        }
    }
}

// ── Setup screen ──────────────────────────────────────────────────────────────

#[component]
fn SetupScreen(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
) -> Element {
    let sites = config.read().sites();
    rsx! {
        div { class: "setup-screen",
            div { class: "setup-card",
                h1 { "PublishKit" }
                p { class: "setup-subtitle",
                    "Select your site's folder. You can configure the LLM provider afterwards."
                }
                if !sites.is_empty() {
                    div { class: "setup-sites",
                        for site in sites {
                            button {
                                class: "setup-site",
                                title: "{site}",
                                onclick: {
                                    let site = site.clone();
                                    move |_| change_site(config, posts, selected, editor_mode, SiteChange::Open(site.clone()))
                                },
                                span { class: "setup-site-name", "{site_label(&site)}" }
                                span { class: "setup-site-path", "{site}" }
                            }
                        }
                    }
                }
                p { class: "setup-hint",
                    "Hugo, Jekyll, Astro, Eleventy, Zola, Docusaurus and Next.js sites are recognised, \
                     as is a plain folder of markdown posts or "
                    code { "content/{{lang}}/{{queue,blog}}" }
                    ". Where it guessed wrong, change it in ⚙ Settings."
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
                                let path = path.path().to_string_lossy().to_string();
                                change_site(config, posts, selected, editor_mode, SiteChange::Open(path));
                            }
                        });
                    },
                    "Open a site's folder"
                }
            }
        }
    }
}

pub(crate) enum SiteChange {
    Open(String),
    /// Off the list, and closed if it is open.
    Forget(String),
}

/// Opens another site, or drops one from the list: what was open of the last one closes, and
/// the posts of the one now open are listed.
pub(crate) fn change_site(
    mut config: Signal<AppConfig>,
    mut posts: Signal<ProjectPosts>,
    mut selected: Signal<Option<Post>>,
    mut editor_mode: Signal<bool>,
    change: SiteChange,
) {
    selected.set(None);
    editor_mode.set(false);
    let mut cfg = config.peek().clone();
    match change {
        SiteChange::Open(path) => cfg.open_project(&path),
        SiteChange::Forget(path) => cfg.forget_project(&path),
    }
    cfg.save();
    posts.set(cfg.project_path.as_deref().map(load_posts).unwrap_or_else(ProjectPosts::empty));
    config.set(cfg);
}

/// The open site and the others PublishKit knows, to switch between; and a way to add one.
#[component]
fn SiteSwitcher(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
    active_lang: Signal<String>,
    notice: Signal<String>,
) -> Element {
    // Remade after each pick, so a cancelled "Open…" shows the open site again.
    let mut picks = use_signal(|| 0u32);
    let current = config.read().project_path.clone().unwrap_or_default();
    let sites = config.read().sites();
    let several = sites.len() > 1;
    let mut switch = move |change: SiteChange| {
        change_site(config, posts, selected, editor_mode, change);
        active_lang.set(String::new());
        notice.set(String::new());
    };
    rsx! {
        select {
            key: "{picks}",
            class: "site-switcher",
            title: "{current}",
            onchange: move |e: Event<FormData>| {
                let value = e.value();
                picks += 1;
                // A site is an absolute path, so these two never name one.
                match value.as_str() {
                    ":open" => {
                        spawn(async move {
                            if let Some(folder) = rfd::AsyncFileDialog::new().set_title("Open a site's folder").pick_folder().await {
                                switch(SiteChange::Open(folder.path().to_string_lossy().to_string()));
                            }
                        });
                    }
                    ":forget" => {
                        let current = config.peek().project_path.clone().unwrap_or_default();
                        switch(SiteChange::Forget(current));
                    }
                    path => switch(SiteChange::Open(path.to_string())),
                }
            },
            for site in sites {
                option { value: "{site}", selected: site == current, "{site_label(&site)}" }
            }
            option { value: ":open", "Open another site…" }
            if several {
                option { value: ":forget", "Remove this site from the list" }
            }
        }
    }
}

/// The api0 region posts are written in, flag and name, and a click to change it right here.
#[component]
fn ModelSwitcher(config: Signal<AppConfig>) -> Element {
    let mut open = use_signal(|| false);
    let region = config.read().provider.region();
    rsx! {
        div { class: "model-switcher",
            button {
                class: "btn-small model-current",
                title: "The model posts are written with \u{2014} click to change it",
                onclick: move |_| open.toggle(),
                match region {
                    Some(r) => rsx! {
                        crate::api0::RegionFlag { id: r.id }
                        span { "{r.label}" }
                    },
                    None => rsx! { span { "{config.read().provider.label()}" } },
                }
                span { class: "model-caret", "\u{25BE}" }
            }
            if open() {
                div { class: "model-backdrop", onclick: move |_| open.set(false) }
                div { class: "model-menu",
                    for r in crate::api0::REGIONS {
                        button {
                            key: "{r.id}",
                            class: if region == Some(r) { "model-option active" } else { "model-option" },
                            title: "api0, in {r.label}",
                            onclick: move |_| {
                                let mut cfg = config.write();
                                cfg.provider = LlmProvider::api0(r.id);
                                cfg.save();
                                drop(cfg);
                                open.set(false);
                            },
                            crate::api0::RegionFlag { id: r.id }
                            span { "{r.label}" }
                        }
                    }
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
    let active_lang = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let site_key = config.read().project_path.clone().unwrap_or_default();
    // The post list's width, dragged by its edge.
    let mut left_width = use_signal(|| config.peek().left_panel_width.unwrap_or(LEFT_PANEL_DEFAULT));
    let mut resizing = use_signal(|| false);
    let mut stop_resizing = move || {
        if resizing() {
            resizing.set(false);
            let mut cfg = config.write();
            cfg.left_panel_width = Some(left_width());
            cfg.save();
        }
    };
    let mut deploying = use_signal(|| false);
    // Scheduled drafts go out while the window is open, those of every site in the list;
    // `publishkit publish-due --all` does it from cron or launchd when it isn't.
    use_future(move || async move {
        loop {
            let sites = config.peek().sites();
            let open = config.peek().project_path.clone();
            for project in sites {
                let done = tokio::task::spawn_blocking({
                    let project = project.clone();
                    move || api::publish_due(&project)
                })
                .await
                .unwrap_or_default();
                if done.is_empty() {
                    continue;
                }
                let site = if open.as_deref() == Some(project.as_str()) { String::new() } else { format!("{}: ", site_label(&project)) };
                let lines: Vec<String> = done.iter().map(|(post, result)| match result {
                    Ok(published) => format!("{site}Scheduled: {}", published.summary()),
                    Err(e) => format!("{site}Scheduled {post} couldn't be published: {e:#}"),
                }).collect();
                notice.set(lines.join(" "));
                if site.is_empty() {
                    let mut posts = posts;
                    posts.set(load_posts(&project));
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
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
                span { class: "topbar-title", "PublishKit" }
                SiteSwitcher { config, posts, selected, editor_mode, active_lang, notice }
                {
                    let p = config.read().project_path.clone().unwrap_or_default();
                    let site = ProjectConfig::load(&p);
                    rsx! {
                        if let Some(preset) = site.preset {
                            span { class: "provider-badge", title: "How PublishKit reads this site — change it in ⚙ Settings", "{preset.label()}" }
                        }
                    }
                }
                div { class: "mode-tabs",
                    button {
                        class: if *content_mode.read() == ContentMode::Blog { "tab active" } else { "tab" },
                        onclick: move |_| *content_mode.write() = ContentMode::Blog,
                        "Blog"
                    }
                    button {
                        class: if *content_mode.read() == ContentMode::Board { "tab active" } else { "tab" },
                        onclick: move |_| *content_mode.write() = ContentMode::Board,
                        "Board"
                    }
                    button {
                        class: if *content_mode.read() == ContentMode::Calendar { "tab active" } else { "tab" },
                        onclick: move |_| *content_mode.write() = ContentMode::Calendar,
                        "Calendar"
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
                ModelSwitcher { config }
                crate::pro::ProButton {}
                button {
                    class: "btn-small",
                    onclick: move |_| {
                        let p = config.read().project_path.clone().unwrap_or_default();
                        *posts.write() = load_posts(&p);
                    },
                    "↻"
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
                {
                    let p = config.read().project_path.clone().unwrap_or_default();
                    let command = ProjectConfig::load(&p).deploy_command;
                    rsx! {
                        button {
                            class: "btn-small btn-deploy",
                            disabled: deploying(),
                            title: if command.is_empty() { "Set the command that puts your site online in ⚙ Settings".to_string() } else { format!("Runs {command}") },
                            onclick: move |_| {
                                let project = config.read().project_path.clone().unwrap_or_default();
                                let pcfg = ProjectConfig::load(&project);
                                if pcfg.deploy_command.trim().is_empty() {
                                    notice.set("Set the command that puts your site online in ⚙ Settings → After publishing / Deploy.".to_string());
                                    show_settings.set(true);
                                    return;
                                }
                                deploying.set(true);
                                notice.set(format!("Deploying with {}…", pcfg.deploy_command));
                                spawn(async move {
                                    let run = tokio::task::spawn_blocking(move || api::deploy(&project, &pcfg)).await.ok().flatten();
                                    notice.set(match run {
                                        Some(run) if run.ok => format!("Deployed: {} ran.", run.command),
                                        Some(run) => format!("The deploy failed: {}", run.output),
                                        None => String::new(),
                                    });
                                    deploying.set(false);
                                });
                            },
                            if deploying() { "Deploying…" } else { "Deploy" }
                        }
                    }
                }
                button {
                    class: "btn-small",
                    title: "Use PublishKit from Claude Desktop or Claude Code",
                    onclick: move |_| show_mcp.set(true),
                    "MCP"
                }
                // A link, not a button: the webview hands it to the browser.
                a {
                    class: "btn-small",
                    href: "{crate::feedback::contact_url()}",
                    target: "_blank",
                    title: "Ideas, bugs, questions: send them to the person who makes PublishKit",
                    "Feedback"
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

            div {
                class: if resizing() { "panels resizing" } else { "panels" },
                style: "--left-width: {left_width}px",
                onmousemove: move |e| {
                    if resizing() {
                        let x = e.client_coordinates().x;
                        left_width.set(x.clamp(LEFT_PANEL_MIN, LEFT_PANEL_MAX).round());
                    }
                },
                onmouseup: move |_| stop_resizing(),
                onmouseleave: move |_| stop_resizing(),
                match *content_mode.read() {
                    ContentMode::Blog => rsx! {
                        // Made anew for each site: no search or half-written post carries over.
                        PostList { key: "{site_key}", config, posts, selected, active_lang, notice }
                        PanelResizer { resizing, left_width, config }
                        CenterPanel { key: "{site_key}", config, posts, selected, editor_mode }
                        PreviewPanel { selected, config }
                    },
                    ContentMode::Board => rsx! {
                        crate::views::Board { config, posts, selected, editor_mode, content_mode, active_lang, notice }
                    },
                    ContentMode::Calendar => rsx! {
                        crate::views::Calendar { posts, selected, editor_mode, content_mode, active_lang }
                    },
                    ContentMode::LinkedIn => rsx! {
                        LinkedInList { config, posts, li_selected }
                        PanelResizer { resizing, left_width, config }
                        LinkedInCenterPanel { config, posts }
                        LinkedInPreviewPanel { li_selected }
                    },
                }
            }

            // Settings modal overlay
            crate::pro::ProWindow {}
            if *show_settings.read() {
                SettingsModal { config, show_settings, posts }
            }
            if show_mcp() {
                crate::mcp::McpModal { show: show_mcp }
            }
        }
    }
}

// ── Settings modal ────────────────────────────────────────────────────────────

#[component]
fn SettingsModal(config: Signal<AppConfig>, show_settings: Signal<bool>, posts: Signal<ProjectPosts>) -> Element {
    let cfg = config.read().clone();

    let mut selected_provider = use_signal(|| cfg.provider.clone());
    let mut accounts = use_signal(|| cfg.crosspost.clone());
    let mut saved_msg = use_signal(|| String::new());
    // What each api0 region costs, asked once when Settings opens.
    let mut offers = use_signal(|| Option::<Result<Vec<crate::api0::RegionOffer>, String>>::None);
    use_future(move || async move {
        offers.set(Some(crate::api0::regions().await));
    });
    let has_licence = crate::api0::licence().is_some();
    let region_notes: Vec<(&'static crate::api0::Region, String)> = crate::api0::REGIONS
        .iter()
        .map(|r| {
            let note = match offers.read().as_ref() {
                _ if !has_licence => "needs a licence".to_string(),
                None => "\u{2026}".to_string(),
                Some(Err(_)) => "unavailable".to_string(),
                Some(Ok(list)) => match list.iter().find(|o| o.id == r.id) {
                    Some(o) => {
                        let n = o.credits_per_post();
                        format!("\u{2248} {n} {} a post", if n == 1 { "credit" } else { "credits" })
                    }
                    None => "not offered".to_string(),
                },
            };
            (r, note)
        })
        .collect();
    let region = selected_provider.read().region();
    let offers_error = match offers.read().as_ref() {
        Some(Err(e)) if has_licence => Some(e.clone()),
        _ => None,
    };

    // This site's settings, kept in its publishkit.yaml.
    let project = cfg.project_path.clone().unwrap_or_default();
    let site = use_hook({
        let project = project.clone();
        move || ProjectConfig::load(&project)
    });
    let languages = site.languages();
    let mut site_name    = use_signal(|| site.site_name.clone());
    let mut site_url     = use_signal(|| site.site_url.clone());
    let mut author       = use_signal(|| site.author.clone());
    let mut tags         = use_signal(|| site.default_tags.join(", "));
    let mut brief        = use_signal(|| site.system_prompt.clone());
    let mut preset_key   = use_signal(|| site.preset.map(|p| p.key().to_string()).unwrap_or_default());
    let mut publish_cmd  = use_signal(|| site.publish_command.clone());
    let mut deploy_cmd   = use_signal(|| site.deploy_command.clone());
    let mut deploy_after = use_signal(|| site.deploy_after_publish);
    let mut post_url     = use_signal(|| site.post_url.clone());
    let mut layout       = use_signal(|| layout_hint(&site));
    let mut images_dir   = use_signal(|| site.images(&project).0);
    let mut images_url   = use_signal(|| site.images(&project).1);
    let mut lang_code    = use_signal(|| languages.first().cloned().unwrap_or_default());
    let single_lang = languages.len() == 1;
    let old_lang = languages.first().cloned().unwrap_or_default();

    rsx! {
        // backdrop
        div {
            class: "modal-backdrop",
            onclick: move |_| *show_settings.write() = false,
        }
        div { class: "modal settings-modal",
            div { class: "modal-head",
                h2 { "Settings" }
                button { class: "modal-close", title: "Close", onclick: move |_| *show_settings.write() = false, "×" }
            }

            div { class: "modal-body",
                // The model: api0's regions, one click each.
                div { class: "settings-field",
                    label { "Model" }
                    div { class: "region-row",
                        for (r, note) in region_notes {
                            button {
                                key: "{r.id}",
                                class: if region == Some(r) { "region active" } else { "region" },
                                title: "api0, in {r.label}",
                                onclick: move |_| *selected_provider.write() = LlmProvider::api0(r.id),
                                crate::api0::RegionFlag { id: r.id }
                                span { class: "region-label", "{r.label}" }
                                span { class: "region-note", "{note}" }
                            }
                        }
                    }
                }

                if let Some(r) = region {
                    div { class: "settings-field",
                        p { class: "settings-hint",
                            if has_licence {
                                "api0 runs the model in {r.label} and takes AI credits from the licence \
                                 shared by every Mayorana app on this computer, with no key to set up. \
                                 Which model runs there is api0's choice. A post is about \
                                 {crate::api0::TYPICAL_POST_TOKENS} tokens; a translation is two calls."
                            } else {
                                "api0 needs a licence. Paste your licence number in any Mayorana app \
                                 (GitAgent, JobScout…) and PublishKit uses it too, or set the "
                                code { "{crate::api0::KEY_ENV}" }
                                " environment variable."
                            }
                        }
                        if let Some(e) = offers_error {
                            p { class: "settings-hint", "{e}" }
                        }
                    }
                }

                h3 { class: "settings-section", "This site" }

                div { class: "settings-field",
                    label { "Built with" }
                    select {
                        onchange: {
                            let project = project.clone();
                            move |e: Event<FormData>| {
                                let key = e.value();
                                if let Some(preset) = Preset::from_key(&key) {
                                    let mut preview = ProjectConfig::load(&project);
                                    preview.apply_preset(preset, &project, false);
                                    publish_cmd.set(preview.publish_command.clone());
                                    post_url.set(preview.post_url.clone());
                                    layout.set(layout_hint(&preview));
                                    if preview.images_dir.is_empty() {
                                        let (dir, url) = preview.images(&project);
                                        images_dir.set(dir);
                                        images_url.set(url);
                                    }
                                }
                                preset_key.set(key);
                            }
                        },
                        if preset_key.read().is_empty() {
                            option { value: "", selected: true, "Not recognised yet" }
                        }
                        for preset in Preset::ALL {
                            option { value: preset.key(), selected: preset_key() == preset.key(), "{preset.label()}" }
                        }
                    }
                    p { class: "settings-hint", "{layout}" }
                }

                div { class: "settings-field",
                    label { "Site name" }
                    input { value: "{site_name}", oninput: move |e| site_name.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "Site address" }
                    input { placeholder: "https://example.com", value: "{site_url}", oninput: move |e| site_url.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "A post's address" }
                    input { value: "{post_url}", oninput: move |e| post_url.set(e.value()) }
                    p { class: "settings-hint",
                        "Used in the social teasers, and as the original's address on cross-posted copies. "
                        code { "{{site_url}}" } " " code { "{{lang}}" } " " code { "{{slug}}" } " "
                        code { "{{year}}" } " " code { "{{month}}" } " " code { "{{day}}" } " are filled in."
                    }
                }
                if single_lang {
                    div { class: "settings-field",
                        label { "Language" }
                        input { placeholder: "en", value: "{lang_code}", oninput: move |e| lang_code.set(e.value()) }
                        p { class: "settings-hint", "The code of the language posts are written in, such as en, fr or de." }
                    }
                }
                div { class: "settings-field",
                    label { "Author" }
                    input { value: "{author}", oninput: move |e| author.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "Default tags" }
                    input { placeholder: "comma, separated", value: "{tags}", oninput: move |e| tags.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "Writing brief" }
                    textarea { rows: "5", value: "{brief}", oninput: move |e| brief.set(e.value()) }
                    p { class: "settings-hint", "What the model is told about your site, its readers and its tone." }
                }
                div { class: "settings-field",
                    label { "Images" }
                    div { class: "meta-row",
                        div { class: "field meta-tags",
                            input { placeholder: "public/images", value: "{images_dir}", oninput: move |e| images_dir.set(e.value()) }
                        }
                        div { class: "field meta-tags",
                            input { placeholder: "/images", value: "{images_url}", oninput: move |e| images_url.set(e.value()) }
                        }
                    }
                    p { class: "settings-hint", "The folder images added to a post are copied into, and the address the site serves it at." }
                }
                div { class: "settings-field",
                    label { "After publishing, run" }
                    input { placeholder: "e.g. npm run build", value: "{publish_cmd}", oninput: move |e| publish_cmd.set(e.value()) }
                    p { class: "settings-hint", "A command run in the site's folder each time a post is published. Leave empty for none." }
                }
                div { class: "settings-field",
                    label { "Deploy" }
                    input {
                        placeholder: "e.g. git add -A && git commit -m \"New post\" && git push",
                        value: "{deploy_cmd}",
                        oninput: move |e| deploy_cmd.set(e.value()),
                    }
                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: deploy_after(),
                            onchange: move |e| deploy_after.set(e.checked()),
                        }
                        "Deploy after each publish, scheduled ones included"
                    }
                    p { class: "settings-hint", "What puts the site online, run in its folder by the Deploy button: npm run deploy, vercel --prod, a git push to a host that builds on push…" }
                }

                h3 { class: "settings-section", "Cross-posting" }
                p { class: "settings-hint",
                    "Send a published post to these platforms from its editor, with your site's address as \
                     canonical so search engines credit the original. The keys stay on this computer."
                }
                div { class: "settings-field",
                    label { "dev.to API key" }
                    input { r#type: "password", placeholder: "dev.to → Settings → Extensions", value: "{accounts.read().devto_key}",
                        oninput: move |e| accounts.write().devto_key = e.value() }
                }
                div { class: "settings-field",
                    label { "Hashnode" }
                    div { class: "meta-row",
                        div { class: "field meta-tags",
                            input { r#type: "password", placeholder: "Personal access token", value: "{accounts.read().hashnode_token}",
                                oninput: move |e| accounts.write().hashnode_token = e.value() }
                        }
                        div { class: "field meta-tags",
                            input { placeholder: "Publication ID", value: "{accounts.read().hashnode_publication}",
                                oninput: move |e| accounts.write().hashnode_publication = e.value() }
                        }
                    }
                }
                div { class: "settings-field",
                    label { "WordPress" }
                    input { placeholder: "https://your-wordpress-site.com", value: "{accounts.read().wordpress_url}",
                        oninput: move |e| accounts.write().wordpress_url = e.value() }
                    div { class: "meta-row",
                        div { class: "field meta-tags",
                            input { placeholder: "User name", value: "{accounts.read().wordpress_user}",
                                oninput: move |e| accounts.write().wordpress_user = e.value() }
                        }
                        div { class: "field meta-tags",
                            input { r#type: "password", placeholder: "Application password", value: "{accounts.read().wordpress_password}",
                                oninput: move |e| accounts.write().wordpress_password = e.value() }
                        }
                    }
                    p { class: "settings-hint", "An application password, from Users → Profile in WordPress — not the one you log in with." }
                }
                div { class: "settings-field",
                    label { "Ghost" }
                    div { class: "meta-row",
                        div { class: "field meta-tags",
                            input { placeholder: "https://your-site.ghost.io", value: "{accounts.read().ghost_url}",
                                oninput: move |e| accounts.write().ghost_url = e.value() }
                        }
                        div { class: "field meta-tags",
                            input { r#type: "password", placeholder: "Admin API key (id:secret)", value: "{accounts.read().ghost_key}",
                                oninput: move |e| accounts.write().ghost_key = e.value() }
                        }
                    }
                    p { class: "settings-hint", "From Settings → Integrations → Add custom integration in Ghost." }
                }
                div { class: "settings-field",
                    label { "Bluesky" }
                    div { class: "meta-row",
                        div { class: "field meta-tags",
                            input { placeholder: "you.bsky.social", value: "{accounts.read().bluesky_handle}",
                                oninput: move |e| accounts.write().bluesky_handle = e.value() }
                        }
                        div { class: "field meta-tags",
                            input { r#type: "password", placeholder: "App password", value: "{accounts.read().bluesky_password}",
                                oninput: move |e| accounts.write().bluesky_password = e.value() }
                        }
                    }
                    p { class: "settings-hint", "An app password from Bluesky's Settings → Privacy and security → App passwords. Posts a post's short teaser from its preview." }
                }
                div { class: "settings-field",
                    label { "Mastodon" }
                    div { class: "meta-row",
                        div { class: "field meta-tags",
                            input { placeholder: "mastodon.social", value: "{accounts.read().mastodon_instance}",
                                oninput: move |e| accounts.write().mastodon_instance = e.value() }
                        }
                        div { class: "field meta-tags",
                            input { r#type: "password", placeholder: "Access token", value: "{accounts.read().mastodon_token}",
                                oninput: move |e| accounts.write().mastodon_token = e.value() }
                        }
                    }
                    p { class: "settings-hint", "From Preferences → Development → New application, with write:statuses." }
                }
                label { class: "settings-check",
                    input { r#type: "checkbox", checked: accounts.read().as_draft,
                        onchange: move |e| accounts.write().as_draft = e.checked() }
                    "Send copies as drafts, to publish there after a look"
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
                        cfg.crosspost = accounts.read().clone();
                        cfg.save();
                        drop(cfg);

                        if !project.is_empty() {
                            let mut site = ProjectConfig::load(&project);
                            if let Some(preset) = Preset::from_key(&preset_key.read()) {
                                if site.preset != Some(preset) {
                                    site.apply_preset(preset, &project, false);
                                }
                            }
                            site.site_name = site_name.read().trim().to_string();
                            site.site_url = site_url.read().trim().to_string();
                            site.author = author.read().trim().to_string();
                            site.default_tags = tags.read().split(',').map(str::trim)
                                .filter(|t| !t.is_empty()).map(String::from).collect();
                            site.system_prompt = brief.read().trim().to_string();
                            site.publish_command = publish_cmd.read().trim().to_string();
                            site.deploy_command = deploy_cmd.read().trim().to_string();
                            site.deploy_after_publish = deploy_after();
                            site.post_url = post_url.read().trim().to_string();
                            // Kept only when it differs from what the framework would use.
                            let (dir, url) = (images_dir.read().trim().to_string(), images_url.read().trim().to_string());
                            site.images_dir = String::new();
                            site.images_url = String::new();
                            if (dir.clone(), url.clone()) != site.images(&project) {
                                site.images_dir = dir;
                                site.images_url = url;
                            }
                            if single_lang && site.languages() == [old_lang.clone()] {
                                site.rename_language(&old_lang, &lang_code.read());
                            }
                            site.save(&project);
                            layout.set(layout_hint(&site));
                            posts.set(load_posts(&project));
                        }
                        *saved_msg.write() = "Saved ✓".to_string();
                    },
                    "Save"
                }
            }
        }
    }
}

/// Where the posts are, in a line: what a preset found in the site's folder.
fn layout_hint(site: &ProjectConfig) -> String {
    let Some(paths) = &site.content_paths else {
        return "No posts found yet.".to_string();
    };
    let multi = paths.langs.len() > 1;
    paths.langs.iter().map(|(lang, p)| {
        let lang = if multi { format!("{}: ", lang.to_uppercase()) } else { String::new() };
        match site.drafts {
            Drafts::Folder => format!("{lang}drafts in {}, published in {}", p.queue, p.blog),
            Drafts::Flag => format!("{lang}posts in {}, drafts marked draft: true", p.blog),
        }
    }).collect::<Vec<_>>().join(" · ")
}

/// The language shown: the one picked, or the site's first.
pub(crate) fn shown_lang(picked: &str, languages: &[String]) -> String {
    if languages.iter().any(|l| l == picked) {
        picked.to_string()
    } else {
        languages.first().cloned().unwrap_or_else(|| "en".to_string())
    }
}

/// One tab per language of the site; nothing for a site in one language.
#[component]
pub(crate) fn LangTabs(languages: Vec<String>, active: Signal<String>) -> Element {
    let current = shown_lang(&active.read(), &languages);
    rsx! {
        if languages.len() > 1 {
            div { class: "lang-tabs",
                for lang in languages {
                    button {
                        class: if lang == current { "tab active" } else { "tab" },
                        onclick: {
                            let lang = lang.clone();
                            move |_| active.set(lang.clone())
                        },
                        "{lang.to_uppercase()}"
                    }
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
    active_lang: Signal<String>,
    notice: Signal<String>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut tag = use_signal(String::new);
    // A draft whose Publish was held back by its checks, and why.
    let mut held = use_signal(|| None::<(Post, Vec<String>)>);

    let languages = posts.read().languages.clone();
    let lang = shown_lang(&active_lang.read(), &languages);
    let mut shelf = posts.read().blog(&lang);
    let mut tags: Vec<String> = shelf.queue.iter().chain(&shelf.published).flat_map(|p| p.tags.clone()).collect();
    tags.sort_by_key(|t| t.to_lowercase());
    tags.dedup();
    let (q, t) = (query.read().to_lowercase(), tag.read().clone());
    let filtering = !q.trim().is_empty() || !t.is_empty();
    shelf.queue.retain(|p| matches_filter(p, &q, &t));
    shelf.published.retain(|p| matches_filter(p, &q, &t));

    rsx! {
        div { class: "panel panel-left",
            LangTabs { languages, active: active_lang }

            div { class: "list-filters",
                input {
                    r#type: "search",
                    placeholder: "Search posts…",
                    value: "{query}",
                    oninput: move |e| query.set(e.value()),
                }
                if !tags.is_empty() {
                    select {
                        onchange: move |e: Event<FormData>| tag.set(e.value()),
                        option { value: "", selected: t.is_empty(), "All tags" }
                        for name in tags {
                            option { value: "{name}", selected: name == t, "{name}" }
                        }
                    }
                }
            }

            if !notice.read().is_empty() {
                p { class: "list-notice", "{notice}" }
            }
            if let Some((post, problems)) = held() {
                div { class: "list-notice held",
                    strong { "“{post.title}” isn't ready:" }
                    for problem in problems {
                        div { "· {problem}" }
                    }
                    div { class: "held-actions",
                        button {
                            class: "btn-micro",
                            onclick: {
                                let post = post.clone();
                                move |_| {
                                    held.set(None);
                                    selected.set(Some(post.clone()));
                                }
                            },
                            "Fix it"
                        }
                        button {
                            class: "btn-micro btn-delete",
                            onclick: {
                                let post = post.clone();
                                move |_| {
                                    held.set(None);
                                    let project = config.read().project_path.clone().unwrap_or_default();
                                    publish_now(project, post.clone(), notice, posts);
                                }
                            },
                            "Publish anyway"
                        }
                    }
                }
            }

            div { class: "section-label",
                if filtering { "Drafts ({shelf.queue.len()} found)" } else { "Drafts ({shelf.queue.len()})" }
            }
            for post in shelf.queue {
                PostItem {
                    is_selected: selected.read().as_ref().is_some_and(|s| s.path == post.path),
                    post,
                    config, posts, selected, notice, held,
                    is_queue: true,
                }
            }

            div { class: "section-label section-label-published",
                if filtering { "Published ({shelf.published.len()} found)" } else { "Published ({shelf.published.len()})" }
            }
            for post in shelf.published {
                PostItem {
                    is_selected: selected.read().as_ref().is_some_and(|s| s.path == post.path),
                    post,
                    config, posts, selected, notice, held,
                    is_queue: false,
                }
            }
        }
    }
}

/// Publishes a draft off the window's thread (the site's publish command may take a while),
/// then says how it went.
fn publish_now(project: String, post: Post, mut notice: Signal<String>, mut posts: Signal<ProjectPosts>) {
    notice.set(format!("Publishing {}…", post.title));
    spawn(async move {
        let done = tokio::task::spawn_blocking({
            let project = project.clone();
            move || {
                let pcfg = ProjectConfig::load(&project);
                api::publish_post(&project, &post.lang, &post.filename, &pcfg)
            }
        }).await;
        match done {
            Ok(Ok(published)) => notice.set(published.summary()),
            Ok(Err(e)) => notice.set(format!("Couldn't publish: {e:#}")),
            Err(e) => notice.set(format!("Couldn't publish: {e}")),
        }
        posts.set(load_posts(&project));
    });
}

/// Whether a post has every word of the search in its title, description, tags or text, and
/// the tag picked.
fn matches_filter(post: &Post, query: &str, tag: &str) -> bool {
    if !tag.is_empty() && !post.tags.iter().any(|t| t == tag) {
        return false;
    }
    let haystack = || format!("{} {} {} {}", post.title, post.description, post.tags.join(" "), post.body).to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    words.is_empty() || { let h = haystack(); words.iter().all(|w| h.contains(w)) }
}

/// `2026-11-02T09:00` as `Nov 2, 09:00`.
pub(crate) fn when_label(when: &str) -> String {
    crate::plan::parse_when(when).map_or_else(|| when.to_string(), |w| w.format("%b %-d, %H:%M").to_string())
}

#[component]
fn PostItem(
    post: Post,
    is_selected: bool,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    notice: Signal<String>,
    held: Signal<Option<(Post, Vec<String>)>>,
    is_queue: bool,
) -> Element {
    let post_click = post.clone();
    let post_pub   = post.clone();
    let post_del   = post.clone();

    rsx! {
        div {
            class: if is_selected { "post-item selected" } else { "post-item" },
            onclick: move |_| *selected.write() = Some(post_click.clone()),

            div { class: "post-item-title", "{post.title}" }
            if !post.description.is_empty() {
                div { class: "post-item-desc", "{post.description}" }
            }
            div { class: "post-item-date",
                "{post.date}"
                if is_queue && post.stage != crate::plan::Stage::Draft {
                    span { class: "stage-badge", "{post.stage.label()}" }
                }
                if let Some(when) = post.publish_at.as_deref() {
                    span { class: "schedule-badge", title: "Published automatically then", "⏱ {when_label(when)}" }
                }
            }

            if is_queue {
                div { class: "post-item-actions",
                    button {
                        class: "btn-micro btn-publish",
                        onclick: move |e| {
                            e.stop_propagation();
                            let project = config.read().project_path.clone().unwrap_or_default();
                            let pcfg = ProjectConfig::load(&project);
                            let issues = crate::checks::check(&post_pub, &project, &pcfg);
                            let problems: Vec<String> = crate::checks::problems(&issues).iter().map(|i| i.message.clone()).collect();
                            if problems.is_empty() {
                                publish_now(project, post_pub.clone(), notice, posts);
                            } else {
                                held.set(Some((post_pub.clone(), problems)));
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
                            match api::delete_queued(&project, &post_del.lang, &post_del.filename, &pcfg) {
                                Ok(()) => *posts.write() = load_posts(&project),
                                Err(e) => notice.set(format!("Couldn't delete: {e:#}")),
                            }
                        },
                        "✕"
                    }
                }
            }
        }
    }
}

/// The post list's width: by default, and how far it may be dragged.
const LEFT_PANEL_DEFAULT: f64 = 240.0;
const LEFT_PANEL_MIN: f64 = 200.0;
const LEFT_PANEL_MAX: f64 = 640.0;

/// The post list's right edge, dragged to widen it; a double-click puts it back.
#[component]
fn PanelResizer(resizing: Signal<bool>, left_width: Signal<f64>, config: Signal<AppConfig>) -> Element {
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
    let mut lang           = use_signal(String::new);
    let languages          = posts.read().languages.clone();
    let mut generating     = use_signal(|| false);
    let mut status_msg     = use_signal(|| String::new());

    // A post picked opens in the editor; the new-post form keeps only its language.
    use_effect(move || {
        if let Some(post) = selected.read().clone() {
            *lang.write()           = post.lang.clone();
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
                                                    path.split('/').last().unwrap_or("")
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
                crate::editor::PostEditor { config, posts, selected }
            }
        }
    }
}

// ── Right panel — preview ──────────────────────────────────────────────────────

#[component]
fn PreviewPanel(selected: Signal<Option<Post>>, config: Signal<AppConfig>) -> Element {
    let post = selected.read().clone();
    let mut copied = use_signal(|| String::new());
    let mut social_status = use_signal(String::new);
    let mut posting = use_signal(|| false);
    let accounts = config.read().crosspost.clone();
    let networks: Vec<crate::social::Network> =
        crate::social::Network::ALL.into_iter().filter(|n| n.configured(&accounts)).collect();

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

// ── LinkedIn: left panel ─────────────────────────────────────────────────────

#[component]
fn LinkedInList(
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
fn LinkedInItem(
    post: LinkedInPost,
    is_selected: bool,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    li_selected: Signal<Option<LinkedInPost>>,
    is_queue: bool,
) -> Element {
    let post_click = post.clone();
    let post_pub   = post.clone();
    let post_del   = post.clone();

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
                            let pcfg = ProjectConfig::load(&project);
                            if api::publish_linkedin_post(&project, &pcfg, &post_pub.lang, &post_pub.filename).is_ok() {
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

// ── LinkedIn: center panel — generate ────────────────────────────────────────

#[component]
fn LinkedInCenterPanel(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
) -> Element {
    let mut topic       = use_signal(|| String::new());
    let mut angle       = use_signal(|| String::new());
    let lang            = use_signal(String::new);
    let mut generating  = use_signal(|| false);
    let mut status_msg  = use_signal(|| String::new());
    let languages       = posts.read().languages.clone();

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

/// The post as HTML, its own images shown: the web view can't load files from the site's
/// folder, so they are put in the page.
fn preview_html(post: &Post, project: &str) -> String {
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
        let Some(len) = rest[start..].find('"') else { break };
        let src = &rest[start..start + len];
        out.push_str(&rest[..start]);
        out.push_str(&local_image(src, &post.path, project, &images_dir, &images_url).unwrap_or_else(|| src.to_string()));
        rest = &rest[start + len..];
    }
    out.push_str(rest);
    out
}

/// A site image as a `data:` address: by the site's images address, from the folder static
/// files are served from, or beside the post.
fn local_image(src: &str, post: &std::path::Path, project: &str, images_dir: &str, images_url: &str) -> Option<String> {
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
            c.extend(["public", "static", ""].iter().map(|d| root.join(d).join(site_path)));
            c
        }
        None => vec![post.parent()?.join(src)],
    };
    let file = candidates.into_iter().find(|p| p.is_file())?;
    let mime = match file.extension()?.to_str()?.to_lowercase().as_str() {
        "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif",
        "webp" => "image/webp", "svg" => "image/svg+xml", "avif" => "image/avif",
        _ => return None,
    };
    // Past a few megabytes the page would slow down; the box stays empty instead.
    if std::fs::metadata(&file).ok()?.len() > 8 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(&file).ok()?;
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn md_to_html(content: &str) -> String {
    let body = crate::frontmatter::body(content);

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(body, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_finds_a_sites_images_and_leaves_others_alone() {
        let root = std::env::temp_dir().join(format!("publishkit-preview-{}", std::process::id()));
        std::fs::create_dir_all(root.join("public/images/blog")).unwrap();
        std::fs::create_dir_all(root.join("posts/one")).unwrap();
        std::fs::write(root.join("public/images/blog/a.png"), "png").unwrap();
        std::fs::write(root.join("posts/one/b.png"), "png").unwrap();
        let project = root.to_string_lossy().into_owned();
        let post = root.join("posts/one/index.md");
        let data = local_image("/images/blog/a.png", &post, &project, "public/images/blog", "/images/blog").unwrap();
        assert_eq!(data, "data:image/png;base64,cG5n");
        assert!(local_image("b.png?v=2", &post, &project, "images", "/images").is_some());
        assert_eq!(local_image("https://x.org/a.png", &post, &project, "images", "/images"), None);
        assert_eq!(local_image("/missing.png", &post, &project, "images", "/images"), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}

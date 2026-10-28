use crate::api;
use crate::state::{
    content_stamp, load_posts, site_label, AppConfig, Drafts, LinkedInPost, LlmProvider, Post,
    Preset, ProjectConfig, ProjectPosts,
};
use desktop_kit::ui::{Header, Theme};
use dioxus::prelude::*;
use pulldown_cmark::{html, Options, Parser};
mod posts;
use posts::*;
mod panels;
use panels::*;
mod linkedin;
use linkedin::*;
mod settings;
use settings::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ContentMode {
    Blog,
    Board,
    Calendar,
    LinkedIn,
}

// ── Root ─────────────────────────────────────────────────────────────────────

#[component]
pub fn App() -> Element {
    let config = use_signal(AppConfig::load);
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
    let theme = desktop_kit::ui::use_theme();

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
            MainLayout { config, posts, selected, editor_mode, show_settings, content_mode, li_selected, theme }
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
                    ". Where it guessed wrong, change it in Settings."
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
    posts.set(
        cfg.project_path
            .as_deref()
            .map(load_posts)
            .unwrap_or_else(ProjectPosts::empty),
    );
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
                class: "kit-btn kit-model",
                title: "The model posts are written with \u{2014} click to change it",
                onclick: move |_| open.toggle(),
                match region {
                    Some(r) => rsx! {
                        desktop_kit::ui::RegionFlag { id: r.id }
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
                            desktop_kit::ui::RegionFlag { id: r.id }
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
    theme: Theme,
) -> Element {
    let mut show_mcp = use_signal(|| false);
    let active_lang = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let site_key = config.read().project_path.clone().unwrap_or_default();
    // The post list's width, dragged by its edge.
    let mut left_width =
        use_signal(|| config.peek().left_panel_width.unwrap_or(LEFT_PANEL_DEFAULT));
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
                let site = if open.as_deref() == Some(project.as_str()) {
                    String::new()
                } else {
                    format!("{}: ", site_label(&project))
                };
                let lines: Vec<String> = done
                    .iter()
                    .map(|(post, result)| match result {
                        Ok(published) => format!("{site}Scheduled: {}", published.summary()),
                        Err(e) => format!("{site}Scheduled {post} couldn't be published: {e:#}"),
                    })
                    .collect();
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
            let Some(project) = config.peek().project_path.clone() else {
                continue;
            };
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
            Header {
                theme,
                on_settings: move |_| show_settings.toggle(),
                on_mcp: move |_| show_mcp.set(true),
                actions: rsx! {
                    button {
                        class: "kit-btn kit-icon",
                        title: "Read the posts from disk again",
                        onclick: move |_| {
                            let p = config.read().project_path.clone().unwrap_or_default();
                            *posts.write() = load_posts(&p);
                        },
                        "↻"
                    }
                    {
                        let p = config.read().project_path.clone().unwrap_or_default();
                        let command = ProjectConfig::load(&p).deploy_command;
                        rsx! {
                            button {
                                class: "kit-btn kit-deploy",
                                disabled: deploying(),
                                title: if command.is_empty() { "Set the command that puts your site online in Settings".to_string() } else { format!("Runs {command}") },
                                onclick: move |_| {
                                    let project = config.read().project_path.clone().unwrap_or_default();
                                    let pcfg = ProjectConfig::load(&project);
                                    if pcfg.deploy_command.trim().is_empty() {
                                        notice.set("Set the command that puts your site online in Settings → After publishing / Deploy.".to_string());
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
                },
                model: rsx! { ModelSwitcher { config } },
                pro: rsx! { crate::pro::ProButton {} },
                SiteSwitcher { config, posts, selected, editor_mode, active_lang, notice }
                {
                    let p = config.read().project_path.clone().unwrap_or_default();
                    let site = ProjectConfig::load(&p);
                    rsx! {
                        if let Some(preset) = site.preset {
                            span { class: "provider-badge", title: "How PublishKit reads this site — change it in Settings", "{preset.label()}" }
                        }
                    }
                }
                div { class: "kit-tabs",
                    for (mode, name) in [(ContentMode::Blog, "Blog"), (ContentMode::Board, "Board"), (ContentMode::Calendar, "Calendar"), (ContentMode::LinkedIn, "LinkedIn")] {
                        button {
                            key: "{name}",
                            class: if *content_mode.read() == mode { "kit-tab kit-on" } else { "kit-tab" },
                            onclick: move |_| *content_mode.write() = mode,
                            "{name}"
                        }
                    }
                }
                span { class: "kit-detail", { config.read().project_path.as_deref().unwrap_or("") } }
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
                        // (A key counts only on a block's first node, hence the fragment.)
                        Fragment { key: "{site_key}",
                            PostList { config, posts, selected, active_lang, notice }
                            PanelResizer { resizing, left_width, config }
                            CenterPanel { config, posts, selected, editor_mode }
                            PreviewPanel { selected, config }
                        }
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

/// The language shown: the one picked, or the site's first.
pub(crate) fn shown_lang(picked: &str, languages: &[String]) -> String {
    if languages.iter().any(|l| l == picked) {
        picked.to_string()
    } else {
        languages
            .first()
            .cloned()
            .unwrap_or_else(|| "en".to_string())
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

/// `2026-11-02T09:00` as `Nov 2, 09:00`.
pub(crate) fn when_label(when: &str) -> String {
    crate::plan::parse_when(when).map_or_else(
        || when.to_string(),
        |w| w.format("%b %-d, %H:%M").to_string(),
    )
}

/// The post list's width: by default, and how far it may be dragged.
const LEFT_PANEL_DEFAULT: f64 = 240.0;
const LEFT_PANEL_MIN: f64 = 200.0;
const LEFT_PANEL_MAX: f64 = 640.0;

// ── Center panel ──────────────────────────────────────────────────────────────

// ── Right panel — preview ──────────────────────────────────────────────────────

// ── LinkedIn: left panel ─────────────────────────────────────────────────────

// ── LinkedIn: center panel — generate ────────────────────────────────────────

// ── LinkedIn: right panel — preview ──────────────────────────────────────────

pub(crate) fn copy_to_clipboard(text: &str) {
    // Dioxus desktop runs in a webview — use JS clipboard API via document eval
    let escaped = text
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");
    let js = format!("navigator.clipboard.writeText(`{escaped}`)");
    let _ = dioxus::prelude::document::eval(&js);
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
mod tests;

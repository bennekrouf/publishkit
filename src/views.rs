//! Whole-site views of the blog: the board, where drafts move from idea to review and get a
//! date, and the calendar of what went out and what is scheduled. A post opened from either is
//! edited in the Blog tab.

use crate::api;
use crate::plan::{self, Stage};
use crate::screens::{when_label, ContentMode};
use crate::state::{load_posts, AppConfig, Post, ProjectConfig, ProjectPosts};
use chrono::{Datelike, NaiveDate};
use dioxus::prelude::*;
use std::collections::BTreeMap;

/// Published posts the board shows, newest first.
const BOARD_PUBLISHED: usize = 12;

/// Opens a post in the Blog tab's editor, in its language.
fn open(
    post: Post,
    mut selected: Signal<Option<Post>>,
    mut editor_mode: Signal<bool>,
    mut content_mode: Signal<ContentMode>,
    mut active_lang: Signal<String>,
) {
    active_lang.set(post.lang.clone());
    selected.set(Some(post));
    editor_mode.set(true);
    content_mode.set(ContentMode::Blog);
}

#[component]
pub fn Board(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
    content_mode: Signal<ContentMode>,
    active_lang: Signal<String>,
    notice: Signal<String>,
) -> Element {
    let mut idea = use_signal(String::new);
    let idea_lang = use_signal(String::new);

    let all = posts.read().clone();
    let languages = all.languages.clone();
    let multilingual = languages.len() > 1;
    let drafts: Vec<Post> = languages.iter().flat_map(|l| all.blog(l).queue).collect();
    let mut published: Vec<Post> = languages
        .iter()
        .flat_map(|l| all.blog(l).published)
        .collect();
    published.sort_by(|a, b| b.date.cmp(&a.date));
    published.truncate(BOARD_PUBLISHED);
    let mut scheduled: Vec<Post> = drafts
        .iter()
        .filter(|p| p.publish_at.is_some())
        .cloned()
        .collect();
    scheduled.sort_by_key(|p| p.publish_at.clone());
    let column = |stage: Stage| -> Vec<Post> {
        drafts
            .iter()
            .filter(|p| p.publish_at.is_none() && p.stage == stage)
            .cloned()
            .collect()
    };

    let mut add_idea = move || {
        let title = idea.read().trim().to_string();
        if title.is_empty() {
            return;
        }
        let project = config.peek().project_path.clone().unwrap_or_default();
        let pcfg = ProjectConfig::load(&project);
        let lang = crate::screens::shown_lang(&idea_lang.peek(), &pcfg.languages());
        match api::save_to_queue(&project, &title, "", &lang, "", &pcfg) {
            Ok(path) => {
                let filename = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let _ = plan::update(&project, &lang, &filename, |e| e.stage = Stage::Idea);
                idea.set(String::new());
                posts.set(load_posts(&project));
            }
            Err(e) => notice.set(format!("Couldn't add the idea: {e:#}")),
        }
    };

    let columns = [
        (
            "Ideas",
            column(Stage::Idea),
            "Titles to write about. Add one above.",
        ),
        ("Drafts", column(Stage::Draft), "Being written."),
        (
            "Review",
            column(Stage::Review),
            "Written, to read again before it goes out.",
        ),
        (
            "Scheduled",
            scheduled,
            "Published automatically at their time. Set it in the editor.",
        ),
        ("Published", published, "The latest out."),
    ];

    rsx! {
        div { class: "board",
            div { class: "board-bar",
                input {
                    placeholder: "A new idea: its title",
                    value: "{idea}",
                    oninput: move |e| idea.set(e.value()),
                    onkeydown: move |e| if e.key() == Key::Enter { add_idea() },
                }
                if multilingual {
                    crate::screens::LangTabs { languages: languages.clone(), active: idea_lang }
                }
                button { class: "btn-primary", disabled: idea.read().trim().is_empty(), onclick: move |_| add_idea(), "Add idea" }
                if !notice.read().is_empty() {
                    span { class: "board-notice", "{notice}" }
                }
            }
            div { class: "board-columns",
                for (name, cards, hint) in columns {
                    div { class: "board-column",
                        div { class: "board-column-head", "{name} " span { class: "board-count", "{cards.len()}" } }
                        if cards.is_empty() {
                            p { class: "board-empty", "{hint}" }
                        }
                        for post in cards {
                            BoardCard { post, multilingual, config, posts, selected, editor_mode, content_mode, active_lang }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BoardCard(
    post: Post,
    multilingual: bool,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
    content_mode: Signal<ContentMode>,
    active_lang: Signal<String>,
) -> Element {
    let draft = post.is_draft();
    let stage = post.stage;
    let index = Stage::ALL.iter().position(|s| *s == stage).unwrap_or(1);
    let previous = index.checked_sub(1).map(|i| Stage::ALL[i]);
    let next = Stage::ALL.get(index + 1).copied();
    let scheduled = post.publish_at.clone();

    let change = {
        let post = post.clone();
        move |f: Box<dyn FnOnce(&mut plan::Entry)>| {
            let project = config.peek().project_path.clone().unwrap_or_default();
            let _ = plan::update(&project, &post.lang, &post.filename, f);
            let mut posts = posts;
            posts.set(load_posts(&project));
        }
    };
    let opened = post.clone();

    rsx! {
        div {
            class: "board-card",
            onclick: move |_| open(opened.clone(), selected, editor_mode, content_mode, active_lang),
            div { class: "board-card-title", "{post.title}" }
            div { class: "board-card-meta",
                if multilingual {
                    span { class: "preview-lang-badge", "{post.lang.to_uppercase()}" }
                }
                if let Some(when) = scheduled.as_deref() {
                    span { "⏱ {when_label(when)}" }
                } else {
                    span { "{post.date}" }
                }
            }
            if draft {
                div { class: "board-card-actions",
                    if scheduled.is_some() {
                        button {
                            class: "btn-micro",
                            onclick: {
                                let change = change.clone();
                                move |e: Event<MouseData>| { e.stop_propagation(); change(Box::new(|e| e.publish_at = None)); }
                            },
                            "Unschedule"
                        }
                    } else {
                        if let Some(to) = previous {
                            button {
                                class: "btn-micro",
                                title: "Back to {to.label()}",
                                onclick: {
                                    let change = change.clone();
                                    move |e: Event<MouseData>| { e.stop_propagation(); change(Box::new(move |e| e.stage = to)); }
                                },
                                "‹ {to.label()}"
                            }
                        }
                        if let Some(to) = next {
                            button {
                                class: "btn-micro",
                                title: "On to {to.label()}",
                                onclick: {
                                    let change = change.clone();
                                    move |e: Event<MouseData>| { e.stop_propagation(); change(Box::new(move |e| e.stage = to)); }
                                },
                                "{to.label()} ›"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Monday-first day names, for the calendar's head.
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[component]
pub fn Calendar(
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
    editor_mode: Signal<bool>,
    content_mode: Signal<ContentMode>,
    active_lang: Signal<String>,
) -> Element {
    let today = chrono::Local::now().date_naive();
    let mut month = use_signal(|| (today.year(), today.month()));
    let (year, number) = month();
    let first = NaiveDate::from_ymd_opt(year, number, 1).unwrap_or(today);
    let next_first = if number == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, number + 1, 1)
    }
    .unwrap_or(first);
    let offset = first.weekday().num_days_from_monday() as i64;
    let days = (next_first - first).num_days();
    let cells = ((offset + days + 6) / 7) * 7;

    // What went out on each day, and what is due out.
    let all = posts.read().clone();
    let multilingual = all.languages.len() > 1;
    let mut by_day: BTreeMap<NaiveDate, Vec<(Post, bool)>> = BTreeMap::new();
    for lang in &all.languages {
        let shelf = all.blog(lang);
        for post in shelf.published {
            if let Some(day) = post
                .date
                .get(..10)
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            {
                by_day.entry(day).or_default().push((post, false));
            }
        }
        for post in shelf.queue {
            if let Some(when) = post.publish_at.as_deref().and_then(plan::parse_when) {
                by_day.entry(when.date()).or_default().push((post, true));
            }
        }
    }

    let mut step = move |delta: i32| {
        let (y, m) = month();
        let index = y * 12 + m as i32 - 1 + delta;
        month.set((index.div_euclid(12), (index.rem_euclid(12) + 1) as u32));
    };

    rsx! {
        div { class: "calendar",
            div { class: "calendar-bar",
                button { class: "btn-small", onclick: move |_| step(-1), "‹" }
                span { class: "calendar-title", "{first.format(\"%B %Y\")}" }
                button { class: "btn-small", onclick: move |_| step(1), "›" }
                button { class: "btn-small", onclick: move |_| month.set((today.year(), today.month())), "Today" }
                span { class: "settings-hint", "Published posts on their date; ⏱ scheduled drafts on the day they go out." }
            }
            div { class: "calendar-head",
                for name in WEEKDAYS {
                    div { class: "calendar-weekday", "{name}" }
                }
            }
            div { class: "calendar-grid",
                for i in 0..cells {
                    {
                        let day = first + chrono::Duration::days(i - offset);
                        let inside = day.month() == number;
                        let items = by_day.get(&day).cloned().unwrap_or_default();
                        let class = match (inside, day == today) {
                            (_, true) => "calendar-day today",
                            (true, false) => "calendar-day",
                            (false, false) => "calendar-day outside",
                        };
                        rsx! {
                            div { class,
                                div { class: "calendar-day-number", "{day.day()}" }
                                for (post, scheduled) in items {
                                    {
                                        let opened = post.clone();
                                        let lang = if multilingual { format!("{} · ", post.lang.to_uppercase()) } else { String::new() };
                                        let time = post.publish_at.as_deref().and_then(plan::parse_when)
                                            .map(|w| w.format("%H:%M ").to_string()).unwrap_or_default();
                                        rsx! {
                                            div {
                                                class: if scheduled { "calendar-item scheduled" } else { "calendar-item" },
                                                title: "{post.title}",
                                                onclick: move |_| open(opened.clone(), selected, editor_mode, content_mode, active_lang),
                                                if scheduled { "⏱ {time}" }
                                                "{lang}{post.title}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

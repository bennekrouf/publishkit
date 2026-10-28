//! The list of posts on the left.

use super::*;

#[component]
pub(super) fn PostList(
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
    let mut tags: Vec<String> = shelf
        .queue
        .iter()
        .chain(&shelf.published)
        .flat_map(|p| p.tags.clone())
        .collect();
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
pub(super) fn publish_now(
    project: String,
    post: Post,
    mut notice: Signal<String>,
    mut posts: Signal<ProjectPosts>,
) {
    notice.set(format!("Publishing {}…", post.title));
    spawn(async move {
        let done = tokio::task::spawn_blocking({
            let project = project.clone();
            move || {
                let pcfg = ProjectConfig::load(&project);
                api::publish_post(&project, &post.lang, &post.filename, &pcfg)
            }
        })
        .await;
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
pub(super) fn matches_filter(post: &Post, query: &str, tag: &str) -> bool {
    if !tag.is_empty() && !post.tags.iter().any(|t| t == tag) {
        return false;
    }
    let haystack = || {
        format!(
            "{} {} {} {}",
            post.title,
            post.description,
            post.tags.join(" "),
            post.body
        )
        .to_lowercase()
    };
    let words: Vec<&str> = query.split_whitespace().collect();
    words.is_empty() || {
        let h = haystack();
        words.iter().all(|w| h.contains(w))
    }
}

#[component]
pub(super) fn PostItem(
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
    let post_pub = post.clone();
    let post_del = post.clone();

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

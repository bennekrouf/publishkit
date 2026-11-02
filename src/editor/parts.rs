//! Parts of the post editor: images pasted into the text, a draft's schedule, cross-posting a
//! published post, and the checks.

use super::*;
use crate::crosspost::Target;
use crate::state::AppConfig;

/// Images pasted into the body arrive here, as the browser read them; they're written to a
/// file of their own, then handed to `add_images` like a dropped image.
pub(super) fn use_pasted_images(
    mut status: Signal<String>,
    mut add_images: impl FnMut(Vec<PathBuf>) + Copy + 'static,
) {
    use_future(move || async move {
        let mut channel = document::eval(EDITOR_JS);
        loop {
            let pasted = match channel.recv::<serde_json::Value>().await {
                Ok(pasted) => pasted,
                Err(e) => {
                    status.set(format!("The editor's tools didn't start: {e}"));
                    break;
                }
            };
            use base64::Engine;
            let Some(bytes) = pasted["data"]
                .as_str()
                .and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok())
            else {
                continue;
            };
            let extension = match pasted["type"].as_str().unwrap_or("") {
                "image/jpeg" => "jpg",
                "image/gif" => "gif",
                "image/webp" => "webp",
                "image/svg+xml" => "svg",
                _ => "png",
            };
            let dir = std::env::temp_dir().join("publishkit-pasted");
            let _ = std::fs::create_dir_all(&dir);
            let file = dir.join(format!(
                "pasted-{}.{extension}",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            ));
            match std::fs::write(&file, bytes) {
                Ok(()) => add_images(vec![file]),
                Err(e) => status.set(format!("Couldn't keep the pasted image: {e}")),
            }
        }
    });
}

/// A draft's stage and the date it is published on, kept in the site's plan.
pub(super) fn schedule_row(
    stage: Stage,
    publish_at: String,
    mut plan_change: impl FnMut(Box<dyn FnOnce(&mut plan::Entry)>) + Copy + 'static,
) -> Element {
    rsx! {
        div { class: "meta-row",
            div { class: "field",
                label { "Stage" }
                select {
                    onchange: move |e: Event<FormData>| {
                        if let Some(s) = Stage::from_key(&e.value()) {
                            plan_change(Box::new(move |entry| entry.stage = s));
                        }
                    },
                    for s in Stage::ALL {
                        option { value: s.key(), selected: s == stage, "{s.label()}" }
                    }
                }
            }
            div { class: "field meta-tags",
                label { "Publish on" }
                div { class: "schedule-row",
                    input {
                        r#type: "datetime-local",
                        value: "{publish_at}",
                        onchange: move |e: Event<FormData>| {
                            let when = plan::parse_when(&e.value()).map(plan::format_when);
                            plan_change(Box::new(move |entry| entry.publish_at = when));
                        },
                    }
                    if !publish_at.is_empty() {
                        button {
                            class: "btn-small",
                            title: "Don't publish it automatically",
                            onclick: move |_| plan_change(Box::new(|entry| entry.publish_at = None)),
                            "✕"
                        }
                    }
                }
            }
        }
    }
}

/// Sending copies of a published post to dev.to, Hashnode, WordPress or Ghost. What is unsaved
/// is saved first.
pub(super) fn crosspost_row(
    config: Signal<AppConfig>,
    selected: Signal<Option<Post>>,
    mut sending: Signal<Option<Target>>,
    mut status: Signal<String>,
    unsaved: impl Fn() -> bool + Copy + 'static,
    mut save: impl FnMut() + Copy + 'static,
) -> Element {
    use crate::crosspost::Record;
    let project = move || config.peek().project_path.clone().unwrap_or_default();
    let accounts = config.read().crosspost.clone();
    let targets: Vec<Target> = Target::ALL
        .into_iter()
        .filter(|t| t.configured(&accounts))
        .collect();
    let (lang, filename) = selected
        .read()
        .as_ref()
        .map(|p| (p.lang.clone(), p.filename.clone()))
        .unwrap_or_default();
    let copies = Record::load(&project()).copies(&lang, &filename);
    rsx! {
        div { class: "crosspost-row",
            span { class: "crosspost-label", "Cross-post" }
            if targets.is_empty() {
                span { class: "settings-hint", "Add dev.to, Hashnode, WordPress or Ghost in ⚙ Settings to send copies." }
            }
            for target in targets {
                {
                    let copy = copies.get(&target).cloned();
                    let verb = if copy.is_some() { "Update" } else { "Send to" };
                    let how = if accounts.as_draft { "as a draft" } else { "published" };
                    rsx! {
                        button {
                            class: "btn-small",
                            disabled: sending().is_some(),
                            title: "{verb} {target.label()} {how}, with this post's address as canonical",
                            onclick: move |_| {
                                if unsaved() {
                                    save();
                                }
                                let Some(post) = selected.peek().clone() else { return };
                                let project = project();
                                let accounts = config.peek().crosspost.clone();
                                sending.set(Some(target));
                                status.set(format!("Sending to {}…", target.label()));
                                spawn(async move {
                                    let pcfg = ProjectConfig::load(&project);
                                    let content = std::fs::read_to_string(&post.path).unwrap_or(post.full_content.clone());
                                    let article = crate::crosspost::Article::of(
                                        &content, &pcfg.site_url, &pcfg.address(&post.lang, &post.filename, &content),
                                    );
                                    let done = crate::crosspost::crosspost(
                                        &project, &accounts, target, &post.lang, &post.filename, &article,
                                    ).await;
                                    status.set(match done {
                                        Ok(copy) => format!("On {}: {}", target.label(), copy.url),
                                        Err(e) => format!("Couldn't send to {}: {e:#}", target.label()),
                                    });
                                    sending.set(None);
                                });
                            },
                            if sending() == Some(target) { "Sending…" }
                            else if copy.is_some() { "↻ {target.label()}" }
                            else { "{target.label()}" }
                        }
                        if let Some(copy) = copy.filter(|c| !c.url.is_empty()) {
                            a { class: "crosspost-link", href: "{copy.url}", target: "_blank", title: "{copy.url}", "↗" }
                        }
                    }
                }
            }
        }
    }
}

/// What the checks found, the links found broken, and the button that looks for them.
pub(super) fn checks(
    issues: Vec<crate::checks::Issue>,
    body: Signal<String>,
    mut link_issues: Signal<Vec<String>>,
    mut checking_links: Signal<bool>,
    mut status: Signal<String>,
) -> Element {
    rsx! {
        div { class: "checks",
            if issues.is_empty() {
                span { class: "checks-ok", "✓ Nothing to fix" }
            }
            for issue in issues {
                div {
                    class: if issue.level == crate::checks::Level::Problem { "check problem" } else { "check warning" },
                    "{issue.message}"
                }
            }
            for message in link_issues() {
                div { class: "check warning", "{message}" }
            }
            button {
                class: "btn-micro",
                disabled: checking_links(),
                title: "Open each link to another site and report the ones that fail",
                onclick: move |_| {
                    let text = body.peek().clone();
                    checking_links.set(true);
                    link_issues.set(vec![]);
                    spawn(async move {
                        let found = crate::checks::broken_links(&text).await;
                        link_issues.set(if found.is_empty() {
                            vec![]
                        } else {
                            found.into_iter().map(|i| i.message).collect()
                        });
                        status.set(if link_issues.peek().is_empty() { "Every link answers.".to_string() } else { String::new() });
                        checking_links.set(false);
                    });
                },
                if checking_links() { "Checking links…" } else { "Check links" }
            }
        }
    }
}

#[cfg(test)]
mod tests;

//! What the main window keeps doing in the background, and its Deploy button.

use super::*;
mod deploy;

/// Scheduled drafts go out while the window is open, those of every site in the list;
/// `publishkit publish-due --all` does it from cron or launchd when it isn't.
pub(super) fn use_publish_due(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    mut notice: Signal<String>,
) {
    use_future(move || async move {
        // Social drafts already reminded of, this session.
        let mut reminded = std::collections::HashSet::new();
        loop {
            // A LinkedIn, X, Instagram or TikTok draft whose planned time has come: PublishKit
            // doesn't post it, so it says so, here and in a notification.
            let due = social_due(&posts.peek(), chrono::Local::now().naive_local());
            let fresh: Vec<SocialPost> = due
                .into_iter()
                .filter(|p| reminded.insert(p.path.clone()))
                .collect();
            if let Some(first) = fresh.first() {
                let more = match fresh.len() {
                    1 => String::new(),
                    n => format!(" (and {} more)", n - 1),
                };
                let line = format!(
                    "Time to post on {}: {}{more}. Post now is in its preview.",
                    first.channel.label(),
                    first.topic
                );
                notify("PublishKit", &line);
                notice.set(line);
            }
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
}

/// What Claude changes through `publishkit mcp` shows up in the lists; a recording PolishCast
/// sent opens Repurpose, and a post opened from another app is shown with `open_target`.
pub(super) fn use_outside_changes(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    mut show_repurpose: Signal<bool>,
    mut handoff: Signal<Option<crate::repurpose::Handoff>>,
    open_target: impl Fn(String) + Copy + 'static,
) {
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
            if !*show_repurpose.peek() {
                if let Some(sent) = tokio::task::spawn_blocking(crate::repurpose::take_handoff)
                    .await
                    .ok()
                    .flatten()
                {
                    handoff.set(Some(sent));
                    show_repurpose.set(true);
                }
            }
            if let Some(note) =
                tokio::task::spawn_blocking(|| desktop_kit::handoff::take("publishkit", "open"))
                    .await
                    .ok()
                    .flatten()
            {
                if let Some(target) = note["open"].as_str() {
                    open_target(target.to_string());
                }
            }
            let changed = last.as_ref().is_none_or(|l| *l != stamp);
            if last.as_ref().is_some_and(|l| *l != stamp) {
                let mut posts = posts;
                posts.set(load_posts(&project));
            }
            // The posts in the library the other apps read, whenever they change.
            if changed {
                let all = posts.peek().clone();
                let _ =
                    tokio::task::spawn_blocking(move || crate::library::sync(&project, &all)).await;
            }
            last = Some(stamp);
        }
    });
}

/// The Deploy button, which asks first in a dialog of its own: see `deploy`. A folder of social
/// posts only has no site to put online.
pub(super) fn deploy_button(
    site: &ProjectConfig,
    config: Signal<AppConfig>,
    deploying: Signal<bool>,
    notice: Signal<String>,
    show_settings: Signal<bool>,
) -> Element {
    let command = site.deploy_command.clone();
    rsx! {
        if site.blog {
            deploy::DeployControl { config, deploying, notice, show_settings, command }
        }
    }
}

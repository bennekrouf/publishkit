//! PublishKit's side of the media library every Mayorana app shares: its posts, so PolishCast
//! and TrackCut show them on their calendars, and the recordings that went into them, so a
//! video attached to a draft shows as queued there, and as posted once the post is.

use crate::state::{Channel, Post, ProjectConfig, ProjectPosts, SocialPost};
use desktop_kit::library::{self as shared, Item, Kind, Link, Role, Status};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Published posts older than this stay off the library: its calendar is for what is coming
/// and what went out lately, not the site's archive.
const PUBLISHED_DAYS: i64 = 365;

/// The library's id for a post: the same once it is published and its file moves.
fn id_of(project: &str, kind: &str, lang: &str, filename: &str) -> String {
    format!("publishkit:{project}::{kind}::{lang}::{filename}")
}

fn date_of(when: Option<crate::plan::When>) -> String {
    match when {
        Some(w) => match w.time {
            Some(t) => format!("{}T{}", w.day.format("%Y-%m-%d"), t.format("%H:%M")),
            None => w.day.format("%Y-%m-%d").to_string(),
        },
        None => String::new(),
    }
}

fn recent(when: Option<crate::plan::When>) -> bool {
    let since = chrono::Local::now().date_naive() - chrono::Duration::days(PUBLISHED_DAYS);
    when.is_some_and(|w| w.day >= since)
}

/// What a draft is called where it is linked from: "LinkedIn draft: Launch day".
fn label_of(channel: Option<Channel>, title: &str, out: bool) -> String {
    let what = match (channel, out) {
        (Some(c), false) => format!("{} draft", c.label()),
        (Some(c), true) => format!("{} post", c.label()),
        (None, false) => "Blog draft".to_string(),
        (None, true) => "Blog post".to_string(),
    };
    format!("{what}: {title}")
}

fn blog_item(project: &str, post: &Post, out: bool) -> Item {
    let (status, when) = if out {
        (Status::Published, post.when)
    } else {
        (
            Status::Queued,
            post.publish_at
                .as_deref()
                .and_then(crate::plan::When::parse),
        )
    };
    let mut item = Item::new(
        id_of(project, "blog", &post.lang, &post.filename),
        "publishkit",
        Kind::Blog,
        &post.title,
        status,
    )
    .with_file(&post.path, Role::Text);
    item.date = date_of(when);
    item.tags = post.tags.clone();
    item.open = post.path.to_string_lossy().into_owned();
    item
}

fn social_item(project: &str, post: &SocialPost, out: bool) -> Item {
    let (status, when) = if out {
        (Status::Posted, post.when)
    } else {
        (
            Status::Queued,
            post.publish_at
                .as_deref()
                .and_then(crate::plan::When::parse),
        )
    };
    let mut item = Item::new(
        id_of(project, post.channel.key(), &post.lang, &post.filename),
        "publishkit",
        Kind::Social,
        &post.topic,
        status,
    )
    .with_file(&post.path, Role::Text);
    for name in &post.media {
        let path = post.media_path(name);
        let role = match Kind::of(&path) {
            Some(Kind::Video) => Role::Video,
            Some(Kind::Image) => Role::Image,
            _ => Role::Other,
        };
        item.files.push(shared::File { path, role });
    }
    item.network = Some(post.channel.key().to_string());
    item.date = date_of(when);
    item.open = post.path.to_string_lossy().into_owned();
    item
}

/// Writes the site's posts into the library, takes out the ones gone from it, and brings the
/// recordings linked to its drafts up to date: a draft published since is followed to its new
/// file, and the recording counts as posted. Reads and writes files: run it off the UI thread.
pub fn sync(project: &str, posts: &ProjectPosts) {
    let config = ProjectConfig::load(project);
    let mut keep = HashSet::new();
    // Each post's file, with what to call it and how far it is; and where a published social
    // post was while it was a draft.
    let mut by_path: HashMap<PathBuf, (String, Status)> = HashMap::new();
    let mut moved: HashMap<PathBuf, PathBuf> = HashMap::new();
    // Recordings drafts were repurposed from.
    let mut origins: Vec<(PathBuf, Link, Status)> = Vec::new();

    for lang in &posts.languages {
        let shelf = posts.blog(lang);
        for (post, out) in shelf
            .queue
            .iter()
            .map(|p| (p, false))
            .chain(shelf.published.iter().map(|p| (p, true)))
        {
            let item = blog_item(project, post, out);
            by_path.insert(
                post.path.clone(),
                (label_of(None, &post.title, out), item.status),
            );
            if !out || recent(post.when) {
                keep.insert(item.id.clone());
                let _ = shared::upsert(item);
            }
        }
        for channel in Channel::ALL {
            let (queue, _) = config.resolve_social_paths(project, channel, lang);
            let shelf = posts.social(channel, lang);
            for (post, out) in shelf
                .queue
                .iter()
                .map(|p| (p, false))
                .chain(shelf.published.iter().map(|p| (p, true)))
            {
                let item = social_item(project, post, out);
                let label = label_of(Some(channel), &post.topic, out);
                by_path.insert(post.path.clone(), (label.clone(), item.status));
                if out {
                    moved.insert(queue.join(&post.filename), post.path.clone());
                }
                if let Some(origin) = &post.origin {
                    let link = Link {
                        app: "publishkit".into(),
                        label: label.clone(),
                        target: post.path.to_string_lossy().into_owned(),
                    };
                    origins.push((PathBuf::from(&origin.source), link, item.status));
                }
                if !out || recent(post.when) {
                    keep.insert(item.id.clone());
                    let _ = shared::upsert(item);
                }
            }
        }
    }
    let _ = shared::retain("publishkit", &format!("publishkit:{project}::"), &keep);
    for (source, link, status) in origins {
        let _ = shared::used(&source, link, status);
    }
    follow_links(project, &by_path, &moved);
}

/// The links other apps' items have to this site's drafts, kept true: renamed and moved with a
/// draft that was published, dropped with one that was deleted, and the item marked posted
/// once its post is out.
fn follow_links(
    project: &str,
    by_path: &HashMap<PathBuf, (String, Status)>,
    moved: &HashMap<PathBuf, PathBuf>,
) {
    for mut item in shared::load_all() {
        if item.app == "publishkit" || !item.links.iter().any(|l| l.app == "publishkit") {
            continue;
        }
        let before = item.clone();
        let mut furthest = item.status;
        item.links.retain_mut(|link| {
            if link.app != "publishkit" {
                return true;
            }
            let mut path = PathBuf::from(&link.target);
            if let Some(now) = moved.get(&path) {
                path = now.clone();
                link.target = path.to_string_lossy().into_owned();
            }
            match by_path.get(&path) {
                Some((label, status)) => {
                    link.label = label.clone();
                    furthest = furthest.max(*status);
                    true
                }
                // Gone from this site: deleted. Another site's draft is left alone.
                None => !path.starts_with(project) || path.exists(),
            }
        });
        if furthest > item.status {
            item.status = furthest;
            item.manual = false;
            item.date = shared::today();
        }
        if item != before {
            let _ = shared::save(&item);
        }
    }
}

/// A video or pictures put on a draft: the library items holding them link to it and show as
/// queued. Files that aren't in the library are left alone.
pub fn attached(post: &Path, channel: Channel, sources: &[PathBuf]) {
    let topic = std::fs::read_to_string(post)
        .ok()
        .map(|c| {
            crate::frontmatter::text(&crate::frontmatter::fields(&c), "topic").unwrap_or_default()
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            post.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    let link = Link {
        app: "publishkit".into(),
        label: label_of(Some(channel), &topic, false),
        target: post.to_string_lossy().into_owned(),
    };
    for source in sources.iter().filter(|s| s.parent() != post.parent()) {
        let _ = shared::used(source, link.clone(), Status::Queued);
    }
}

/// Media from PolishCast and TrackCut, and files added by hand, with a day on the calendar.
pub fn calendar_media(items: &[Item]) -> Vec<Item> {
    items
        .iter()
        .filter(|i| i.app != "publishkit" && !i.day().is_empty())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_keeps_its_id_when_it_is_published() {
        let id = id_of("/site", "linkedin", "en", "launch.md");
        assert_eq!(id, "publishkit:/site::linkedin::en::launch.md");
        assert!(id.starts_with("publishkit:/site::"));
        assert_eq!(
            label_of(Some(Channel::LinkedIn), "Launch", false),
            "LinkedIn draft: Launch"
        );
        assert_eq!(label_of(None, "Launch", true), "Blog post: Launch");
    }

    #[test]
    fn dates_carry_the_time_when_there_is_one() {
        let at = crate::plan::When::parse("2026-11-02T09:00");
        assert_eq!(date_of(at), "2026-11-02T09:00");
        assert_eq!(
            date_of(crate::plan::When::parse("2026-11-02")),
            "2026-11-02"
        );
        assert_eq!(date_of(None), "");
    }
}

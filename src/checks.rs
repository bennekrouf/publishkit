//! What to look at before a post goes out. Problems hold publishing back until they're fixed
//! (or the person publishes anyway); warnings are only shown.

use crate::frontmatter;
use crate::state::{Post, ProjectConfig};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Problem,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub level: Level,
    pub message: String,
}

impl Issue {
    fn problem(message: impl Into<String>) -> Self {
        Issue { level: Level::Problem, message: message.into() }
    }
    fn warning(message: impl Into<String>) -> Self {
        Issue { level: Level::Warning, message: message.into() }
    }
}

/// Lengths search engines show in full.
const TITLE_MAX: usize = 70;
const DESCRIPTION_MIN: usize = 50;
const DESCRIPTION_MAX: usize = 160;
/// Below this, a post reads as a stub.
const WORDS_MIN: usize = 150;
/// Left-overs of writing that shouldn't reach readers, matched case-insensitively.
const MARKERS: [&str; 4] = ["todo", "fixme", "lorem ipsum", "xxx"];

/// The post's markdown links and images: `(is_image, alt or text, target)`.
pub fn links(body: &str) -> Vec<(bool, String, String)> {
    let mut found = vec![];
    let mut in_code = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find('[') {
            let image = open > 0 && rest[..open].ends_with('!');
            let after = &rest[open + 1..];
            let Some(close) = after.find("](") else { break };
            let text = &after[..close];
            let target_start = &after[close + 2..];
            let Some(end) = target_start.find(')') else { break };
            let target = target_start[..end].split_whitespace().next().unwrap_or("").to_string();
            found.push((image, text.to_string(), target));
            rest = &target_start[end + 1..];
        }
    }
    found
}

/// Whether `word` appears in `text` as a word of its own, not inside another.
fn has_word(text: &str, word: &str) -> bool {
    let lower = text.to_lowercase();
    lower.match_indices(word).any(|(i, _)| {
        let before = lower[..i].chars().next_back();
        let after = lower[i + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Everything found in a post without going on the internet.
pub fn check(post: &Post, project_path: &str, cfg: &ProjectConfig) -> Vec<Issue> {
    let mut issues = vec![];
    let fields = frontmatter::fields(&post.full_content);
    let body = frontmatter::body(&post.full_content);

    let title = frontmatter::text(&fields, "title").unwrap_or_default();
    if title.trim().is_empty() {
        issues.push(Issue::problem("It has no title."));
    } else if title.chars().count() > TITLE_MAX {
        issues.push(Issue::warning(format!(
            "The title is {} characters; search results cut it after about {TITLE_MAX}.",
            title.chars().count()
        )));
    }

    let description = frontmatter::text(&fields, crate::api::description_key(&post.full_content)).unwrap_or_default();
    let description = description.trim();
    let length = description.chars().count();
    if description.is_empty() {
        issues.push(Issue::warning("It has no description: search results and link previews will make one up."));
    } else if length < DESCRIPTION_MIN {
        issues.push(Issue::warning(format!("The description is short ({length} characters); {DESCRIPTION_MIN}–{DESCRIPTION_MAX} reads best in search results.")));
    } else if length > DESCRIPTION_MAX {
        issues.push(Issue::warning(format!("The description is {length} characters; search results cut it after about {DESCRIPTION_MAX}.")));
    }

    if frontmatter::list(&fields, "tags").is_empty() && !cfg.default_tags.is_empty() {
        issues.push(Issue::warning("It has no tags."));
    }

    let words = body.split_whitespace().count();
    if words == 0 {
        issues.push(Issue::problem("It has no text."));
    } else if words < WORDS_MIN {
        issues.push(Issue::warning(format!("It is {words} words long.")));
    }

    let mut in_code = false;
    let mut last_level = 0usize;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let level = line.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && line[level..].starts_with(' ') {
            if last_level > 0 && level > last_level + 1 {
                issues.push(Issue::warning(format!(
                    "“{}” jumps from a level-{last_level} heading to level {level}.",
                    line[level..].trim()
                )));
            }
            last_level = level;
        }
    }
    let prose: String = {
        let mut in_code = false;
        body.lines()
            .filter(|l| {
                if l.trim_start().starts_with("```") {
                    in_code = !in_code;
                    return false;
                }
                !in_code
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    for marker in MARKERS {
        if has_word(&prose, marker) || has_word(&title, marker) {
            issues.push(Issue::problem(format!("It still says “{}”.", marker.to_uppercase())));
        }
    }

    let (images_dir, images_url) = cfg.images(project_path);
    for (image, text, target) in links(body) {
        if image && text.trim().is_empty() {
            issues.push(Issue::warning(format!("The image {target} has no description (alt text) for screen readers.")));
        }
        if !image || target.contains("://") || target.starts_with("data:") || target.is_empty() {
            continue;
        }
        // An image of the site's: in its images folder, or beside the post.
        let file = match target.strip_prefix(images_url.trim_end_matches('/')) {
            Some(rest) if target.starts_with('/') => Path::new(project_path).join(&images_dir).join(rest.trim_start_matches('/')),
            _ if target.starts_with('/') => continue,
            _ => post.path.parent().unwrap_or(Path::new(project_path)).join(&target),
        };
        let file = file.to_string_lossy().split(['?', '#']).next().unwrap_or_default().to_string();
        if !Path::new(&file).is_file() {
            issues.push(Issue::problem(format!("The image {target} isn't in the site.")));
        }
    }

    issues.sort_by_key(|i| i.level);
    issues
}

/// Only what holds publishing back.
pub fn problems(issues: &[Issue]) -> Vec<&Issue> {
    issues.iter().filter(|i| i.level == Level::Problem).collect()
}

/// The post's links to other sites that don't answer, or answer with an error. On the internet:
/// run when asked, not on every keystroke.
pub async fn broken_links(body: &str) -> Vec<Issue> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent(concat!("PublishKit/", env!("CARGO_PKG_VERSION"), " (link check)"))
        .build()
        .unwrap_or_default();
    let mut targets: Vec<String> = links(body).into_iter().map(|(_, _, t)| t).filter(|t| t.starts_with("http")).collect();
    targets.sort();
    targets.dedup();
    let checks = targets.into_iter().map(|url| {
        let client = client.clone();
        async move {
            // Some sites refuse HEAD; a GET says for sure.
            let status = match client.head(&url).send().await {
                Ok(r) if r.status().is_success() || r.status().is_redirection() => return None,
                _ => client.get(&url).send().await.map(|r| r.status()),
            };
            match status {
                Ok(s) if s.is_success() || s.is_redirection() => None,
                Ok(s) => Some(Issue::warning(format!("{url} answers {s}."))),
                Err(e) if e.is_timeout() => Some(Issue::warning(format!("{url} didn't answer."))),
                Err(_) => Some(Issue::warning(format!("{url} couldn't be reached."))),
            }
        }
    });
    futures_join_all(checks).await.into_iter().flatten().collect()
}

/// Runs the futures side by side, keeping their order.
async fn futures_join_all<F: std::future::Future>(futures: impl IntoIterator<Item = F>) -> Vec<F::Output>
where
    F: Send + 'static,
    F::Output: Send + 'static,
{
    let handles: Vec<_> = futures.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(v) = h.await {
            out.push(v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Stage;

    fn post(project: &Path, content: &str) -> Post {
        let fields = frontmatter::fields(content);
        Post {
            path: project.join("content/en/queue/p.md"),
            filename: "p.md".into(),
            title: frontmatter::text(&fields, "title").unwrap_or_default(),
            description: frontmatter::text(&fields, "description").unwrap_or_default(),
            date: String::new(),
            lang: "en".into(),
            status: "draft".into(),
            tags: frontmatter::list(&fields, "tags"),
            body: frontmatter::body(content).into(),
            full_content: content.into(),
            social: None,
            stage: Stage::Draft,
            publish_at: None,
        }
    }

    #[test]
    fn links_and_images_are_found_outside_code() {
        let body = "See [docs](https://a.org/x \"t\") and ![](/images/a.png).\n```\n[not](a link)\n```\n![alt](b.png)";
        assert_eq!(
            links(body),
            [
                (false, "docs".to_string(), "https://a.org/x".to_string()),
                (true, String::new(), "/images/a.png".to_string()),
                (true, "alt".to_string(), "b.png".to_string()),
            ]
        );
    }

    #[test]
    fn a_post_with_left_overs_and_missing_images_is_held_back() {
        let root = std::env::temp_dir().join(format!("publishkit-checks-{}", std::process::id()));
        std::fs::create_dir_all(root.join("public/images/blog")).unwrap();
        std::fs::write(root.join("public/images/blog/here.png"), "").unwrap();
        let project = root.to_string_lossy().into_owned();
        let cfg = ProjectConfig { preset: Some(crate::state::Preset::PublishKit), ..Default::default() };
        let content = "---\ntitle: \"A\"\n---\n\n# A\n\n#### Deep\n\nTODO: finish. A mastodon todolist is fine.\n\n\
                       ![](/images/blog/here.png) ![x](/images/blog/gone.png)\n\n```\nlet todo = 1;\n```";
        let issues = check(&post(&root, content), &project, &cfg);
        let messages: Vec<&str> = issues.iter().map(|i| i.message.as_str()).collect();
        assert_eq!(problems(&issues).len(), 2, "{messages:#?}");
        assert!(messages.contains(&"It still says “TODO”."));
        assert!(messages.contains(&"The image /images/blog/gone.png isn't in the site."));
        assert!(messages.iter().any(|m| m.contains("jumps from a level-1 heading to level 4")));
        assert!(messages.iter().any(|m| m.contains("/images/blog/here.png has no description")));
        assert!(messages.iter().any(|m| m.starts_with("It has no description")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_finished_post_has_nothing_to_say() {
        let root = std::env::temp_dir();
        let cfg = ProjectConfig::default();
        let words = "word ".repeat(200);
        let content = format!(
            "---\ntitle: \"A good title\"\ndescription: \"{}\"\n---\n\n## One\n\n{words}\n\n### Two\n\n[a](https://x.org)",
            "d".repeat(80)
        );
        assert_eq!(check(&post(&root, &content), &root.to_string_lossy(), &cfg), []);
    }
}

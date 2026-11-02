//! Saving drafts and social posts to the queue, and changing them there.

use super::*;

pub fn save_social_to_queue(
    project_path: &str,
    project_config: &ProjectConfig,
    channel: Channel,
    topic: &str,
    lang: &str,
    body: &str,
) -> Result<String> {
    save_social_with(
        project_path,
        project_config,
        channel,
        topic,
        lang,
        body,
        &[],
    )
}

/// A network's draft with more front matter after its own: where a repurposed one came from.
pub fn save_social_with(
    project_path: &str,
    project_config: &ProjectConfig,
    channel: Channel,
    topic: &str,
    lang: &str,
    body: &str,
    extra: &[(&str, Field)],
) -> Result<String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (queue_dir, _) = project_config.resolve_social_paths(project_path, channel, lang);
    std::fs::create_dir_all(&queue_dir)?;

    let mut fields = vec![
        ("topic", Field::Text(topic.to_string())),
        ("date", Field::Text(today)),
        ("lang", Field::Text(lang.to_string())),
        ("status", Field::Text("draft".into())),
    ];
    fields.extend(extra.iter().cloned());
    let content = frontmatter::build(Format::Yaml, &fields, body);
    let slug = Some(slugify(topic))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "post".into());
    let path = free_path(&queue_dir, &slug, "md");
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

/// Replaces a post's text, its front matter kept.
pub fn set_body(path: &Path, body: &str) -> Result<()> {
    let content = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    std::fs::write(path, frontmatter::with_body(&content, body))
        .with_context(|| format!("{}", path.display()))?;
    Ok(())
}

/// Sets the date of a post, queued or published, blog or social: written as that kind of
/// post keeps it. A blog post's file name keeps its date prefix: links to it may use it.
/// Keeps where a posted LinkedIn, X, Instagram or TikTok post is online, as `url` in its front
/// matter.
pub fn set_url(path: &Path, url: &str) -> Result<()> {
    let content = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    let format = frontmatter::split(&content).map_or(Format::Yaml, |p| p.format);
    let updated = frontmatter::set(&content, "url", &Field::Text(url.to_string()), format);
    std::fs::write(path, updated).with_context(|| format!("{}", path.display()))
}

pub fn set_date(path: &Path, social: bool, when: When, default: Format) -> Result<()> {
    let content = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    let format = frontmatter::split(&content).map_or(default, |p| p.format);
    let date = if social {
        when.social_date()
    } else {
        let previous =
            frontmatter::text(&frontmatter::fields(&content), "date").unwrap_or_default();
        when.blog_date(&previous)
    };
    let updated = frontmatter::set(&content, "date", &Field::Text(date), format);
    std::fs::write(path, updated)?;
    Ok(())
}

pub fn delete_social_queued(
    project_path: &str,
    project_config: &ProjectConfig,
    channel: Channel,
    lang: &str,
    filename: &str,
) -> Result<()> {
    let (queue, _) = project_config.resolve_social_paths(project_path, channel, lang);
    delete_media(&queue.join(filename));
    std::fs::remove_file(queue.join(filename))?;
    crate::plan::forget_social(project_path, channel, lang, filename);
    Ok(())
}

pub fn save_to_queue(
    project_path: &str,
    title: &str,
    summary: &str,
    lang: &str,
    body: &str,
    project_config: &ProjectConfig,
) -> Result<String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (queue_dir, _) = project_config.resolve_paths(project_path, lang);
    std::fs::create_dir_all(&queue_dir)?;

    let mut fields = vec![
        ("title", Field::Text(title.to_string())),
        ("description", Field::Text(summary.to_string())),
        ("date", Field::Text(today)),
    ];
    if !project_config.author.is_empty() {
        fields.push(("author", Field::Text(project_config.author.clone())));
    }
    if !project_config.default_tags.is_empty() {
        fields.push(("tags", Field::List(project_config.default_tags.clone())));
    }
    if project_config.languages().len() > 1 {
        fields.push(("lang", Field::Text(lang.to_string())));
    }
    match project_config.drafts {
        Drafts::Folder => fields.push(("status", Field::Text("draft".into()))),
        Drafts::Flag => fields.push(("draft", Field::Bool(true))),
    }
    let body = if project_config.title_heading {
        format!("# {title}\n\n{body}")
    } else {
        body.to_string()
    };
    let content = frontmatter::build(project_config.front_matter, &fields, &body);
    // Where the file name marks the language: `post.fr.md`, a bundle's `index.fr.md`.
    let extension = match project_config.language_mark(lang) {
        "" => project_config.extension.clone(),
        mark => format!("{mark}.{}", project_config.extension),
    };
    let path = if project_config.bundles {
        free_bundle(&queue_dir, &slugify(title), &extension)
    } else {
        free_path(&queue_dir, &slugify(title), &extension)
    };
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

/// `dir/slug/index.ext` in a folder of its own: `slug-2` and so on when `slug` is taken.
pub(super) fn free_bundle(dir: &Path, slug: &str, extension: &str) -> PathBuf {
    let slug = if slug.is_empty() { "post" } else { slug };
    let extension = if extension.is_empty() {
        "md"
    } else {
        extension
    };
    let folder = (1..)
        .map(|n| {
            if n == 1 {
                dir.join(slug)
            } else {
                dir.join(format!("{slug}-{n}"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(slug));
    folder.join(format!("index.{extension}"))
}

/// How many posts (languages) a bundle folder holds.
pub(super) fn bundle_posts(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| crate::state::is_bundle_index(&e.file_name().to_string_lossy()))
                .count()
        })
        .unwrap_or(0)
}

/// A post's own folder, for one that is a page bundle (`my-post/index.md`).
pub(super) fn bundle_folder(posts_dir: &Path, filename: &str) -> Option<PathBuf> {
    filename
        .split_once('/')
        .map(|(folder, _)| posts_dir.join(folder))
}

/// `dir/slug.ext`, or `dir/slug-2.ext` and so on when that is taken: a new post never
/// replaces one that is there.
pub(super) fn free_path(dir: &Path, slug: &str, extension: &str) -> PathBuf {
    let slug = if slug.is_empty() { "post" } else { slug };
    let extension = if extension.is_empty() {
        "md"
    } else {
        extension
    };
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{slug}.{extension}"))
            } else {
                dir.join(format!("{slug}-{n}.{extension}"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{slug}.{extension}")))
}

/// The sidecar holding a post's teasers: `my-post.social.json` beside `my-post.md`.
pub(super) fn social_path(md_path: &Path) -> PathBuf {
    md_path.with_extension("social.json")
}

pub fn save_social(md_path: &str, social: &SocialPosts) -> Result<()> {
    let json = serde_json::to_string_pretty(social)?;
    std::fs::write(social_path(Path::new(md_path)), json)?;
    Ok(())
}

/// Today, written like `previous`: a date and time when it had one, else a plain date.
pub(super) fn today_like(previous: &str) -> String {
    let now = chrono::Local::now();
    if previous.contains('T') {
        now.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
    } else {
        now.format("%Y-%m-%d").to_string()
    }
}

/// The front matter key a post keeps its description under.
pub fn description_key(content: &str) -> &'static str {
    let fields = frontmatter::fields(content);
    ["description", "summary", "excerpt"]
        .into_iter()
        .find(|k| fields.contains_key(*k))
        .unwrap_or("description")
}

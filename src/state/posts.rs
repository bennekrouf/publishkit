//! Reading the posts of a site from disk.

use super::*;

/// A post's slug: its file name without the extension or a Jekyll date; a bundle's folder name.
pub fn slug_of(filename: &str) -> String {
    if let Some((folder, _)) = filename.split_once('/') {
        return folder.to_string();
    }
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    // `post.fr` is the French `post`.
    let stem = match marked_language(filename) {
        Some(mark) => stem
            .strip_suffix(&format!(".{mark}"))
            .unwrap_or(&stem)
            .to_string(),
        None => stem,
    };
    match stem.get(..11) {
        Some(prefix) if is_date_prefix(prefix) => stem[11..].to_string(),
        _ => stem,
    }
}

/// Every post file of the project with its modification time, sorted: when it differs from the
/// last one, a post was added, changed, moved or deleted, here or by `publishkit mcp`.
pub fn content_stamp(project_path: &str) -> Vec<(PathBuf, std::time::SystemTime)> {
    let config = ProjectConfig::load(project_path);
    let mut dirs = Vec::new();
    for lang in config.languages() {
        let (queue, blog) = config.resolve_paths(project_path, &lang);
        dirs.extend([queue, blog]);
        for channel in Channel::ALL {
            let (queue, published) = config.resolve_social_paths(project_path, channel, &lang);
            dirs.extend([queue, published]);
        }
    }
    dirs.sort();
    dirs.dedup();
    let mut stamp = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.filter_map(|e| e.ok()) {
            if let Ok(modified) = e.metadata().and_then(|m| m.modified()) {
                stamp.push((e.path(), modified));
            }
            // A bundle's post changes inside its folder, which says nothing of it.
            for index in bundle_files(&e.path()) {
                if let Ok(modified) = std::fs::metadata(&index).and_then(|m| m.modified()) {
                    stamp.push((index, modified));
                }
            }
        }
    }
    // Stages and schedules, which Claude may change through `publishkit mcp` too.
    let plan = Path::new(project_path)
        .join(".publishkit")
        .join("plan.json");
    if let Ok(modified) = std::fs::metadata(&plan).and_then(|m| m.modified()) {
        stamp.push((plan, modified));
    }
    stamp.sort();
    stamp
}

pub fn load_posts(project_path: &str) -> ProjectPosts {
    let config = ProjectConfig::load(project_path);
    let mut posts = ProjectPosts {
        languages: config.languages(),
        ..ProjectPosts::empty()
    };
    let plan = Plan::load(project_path);
    let marks = config.language_marks();
    for lang in &posts.languages {
        let (queue, blog) = config.resolve_paths(project_path, lang);
        let own = config.language_mark(lang);
        // Where languages share a folder, each keeps the files with its mark.
        let mine = |p: &Post| {
            let mark = p
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(marked_language)
                .filter(|m| marks.iter().any(|x| x == m));
            mark.unwrap_or("") == own
        };
        let shelf = match config.drafts {
            Drafts::Folder => Shelf {
                queue: read_posts(&queue, lang, true, config.drafts),
                published: read_posts(&blog, lang, false, config.drafts),
            },
            Drafts::Flag => {
                let (queue, published) = read_posts(&blog, lang, false, config.drafts)
                    .into_iter()
                    .partition(Post::is_draft);
                Shelf { queue, published }
            }
        };
        let mut shelf = shelf;
        if !marks.is_empty() {
            shelf.queue.retain(&mine);
            shelf.published.retain(&mine);
        }
        for post in &mut shelf.queue {
            let entry = plan.get(lang, &post.filename);
            post.stage = entry.stage;
            post.publish_at = entry.publish_at;
        }
        posts.blog.insert(lang.clone(), shelf);
        for channel in Channel::ALL {
            let (queue, published) = config.resolve_social_paths(project_path, channel, lang);
            let mut queue = read_social_posts(&queue, channel, lang);
            for post in &mut queue {
                post.publish_at = plan.social_at(channel, lang, &post.filename);
            }
            posts.social.entry(channel).or_default().insert(
                lang.clone(),
                Shelf {
                    queue,
                    published: read_social_posts(&published, channel, lang),
                },
            );
        }
    }
    posts
}

/// The posts of a bundle folder, one per language.
pub(super) fn bundle_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return vec![];
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_bundle_index)
        })
        .collect()
}

/// The posts in a folder: its markdown files, and the `index.md` of each bundle in it. An
/// `index.md` or `_index.md` of the folder itself is its list page, not a post.
pub fn post_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .flat_map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                bundle_files(&path)
            } else if is_post_file(&path)
                && !name.starts_with("index.")
                && !name.starts_with("_index.")
            {
                vec![path]
            } else {
                vec![]
            }
        })
        .collect();
    files.sort();
    files
}

pub(super) fn read_social_posts(dir: &Path, channel: Channel, lang: &str) -> Vec<SocialPost> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut posts: Vec<SocialPost> = entries
        .filter_map(|e| e.ok())
        .filter(|e| is_post_file(&e.path()))
        .filter_map(|e| parse_social_post(e.path(), channel, lang))
        .collect();
    posts.sort_by(|a, b| b.when.cmp(&a.when).then(b.date.cmp(&a.date)));
    posts
}

pub(super) fn parse_social_post(path: PathBuf, channel: Channel, lang: &str) -> Option<SocialPost> {
    let full_content = std::fs::read_to_string(&path).ok()?;
    let filename = path.file_name()?.to_string_lossy().to_string();
    let fields = frontmatter::fields(&full_content);

    let topic = frontmatter::text(&fields, "topic").unwrap_or_else(|| filename.clone());
    let date = frontmatter::text(&fields, "date").unwrap_or_default();
    let status = frontmatter::text(&fields, "status").unwrap_or_else(|| "draft".to_string());
    let body = frontmatter::body(&full_content).to_string();

    Some(SocialPost {
        channel,
        path,
        filename,
        topic,
        lang: lang.to_string(),
        status,
        date,
        when: When::of_post(&fields),
        publish_at: None,
        body,
        media: frontmatter::list(&fields, "media"),
    })
}

pub(super) fn read_posts(dir: &Path, lang: &str, in_queue: bool, drafts: Drafts) -> Vec<Post> {
    let mut posts: Vec<Post> = post_files(dir)
        .into_iter()
        .filter_map(|path| {
            let name = post_name(dir, &path)?;
            parse_post(path, name, lang, in_queue, drafts)
        })
        .collect();
    // Newest first; undated drafts on top.
    posts.sort_by(|a, b| {
        (a.date.is_empty(), &a.date)
            .cmp(&(b.date.is_empty(), &b.date))
            .reverse()
            .then(a.filename.cmp(&b.filename))
    });
    posts
}

pub(super) fn parse_post(
    path: PathBuf,
    filename: String,
    lang: &str,
    in_queue: bool,
    drafts: Drafts,
) -> Option<Post> {
    let full_content = std::fs::read_to_string(&path).ok()?;
    let fields = frontmatter::fields(&full_content);
    let text = |key: &str| frontmatter::text(&fields, key);

    let title = text("title").unwrap_or_else(|| slug_of(&filename));
    let description = text("description")
        .or_else(|| text("summary"))
        .or_else(|| text("excerpt"))
        .unwrap_or_default();
    let date = text("date").unwrap_or_default();
    let tags = frontmatter::list(&fields, "tags");
    let marked = text("status");
    let status = match drafts {
        Drafts::Folder => {
            marked.unwrap_or_else(|| if in_queue { "draft" } else { "published" }.to_string())
        }
        Drafts::Flag
            if frontmatter::flag(&fields, "draft") == Some(true)
                || marked.as_deref() == Some("draft") =>
        {
            "draft".to_string()
        }
        Drafts::Flag => "published".to_string(),
    };
    let body = frontmatter::body(&full_content).to_string();

    // Load sidecar social file if it exists
    let social_path = path.with_extension("social.json");
    let social = std::fs::read_to_string(&social_path)
        .ok()
        .and_then(|s| serde_json::from_str::<SocialPosts>(&s).ok());

    Some(Post {
        path,
        filename,
        title,
        description,
        when: When::of_post(&fields),
        date,
        lang: lang.to_string(),
        status,
        tags,
        body,
        full_content,
        social,
        stage: Stage::Draft,
        publish_at: None,
    })
}

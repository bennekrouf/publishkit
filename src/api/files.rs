//! Images, translations and removing a queued post.

use super::*;

/// Copies an image into the site's images folder, under a name of its own, and gives the
/// address to link it at. A page bundle keeps its images in its own folder, linked by name.
pub fn add_image(
    project_path: &str,
    project_config: &ProjectConfig,
    source: &Path,
    post: &Path,
) -> Result<String> {
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !["png", "jpg", "jpeg", "gif", "webp", "svg", "avif"].contains(&extension.as_str()) {
        anyhow::bail!(
            "{} is not an image PublishKit adds (png, jpg, gif, webp, svg, avif)",
            source.display()
        );
    }
    let stem = source
        .file_stem()
        .map(|s| slugify(&s.to_string_lossy()))
        .unwrap_or_default();
    let stem = if stem.is_empty() {
        "image".to_string()
    } else {
        stem
    };
    if post
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(crate::state::is_bundle_index)
    {
        let folder = post.parent().context("a bundle's post is in a folder")?;
        let target = free_path(folder, &stem, &extension);
        std::fs::copy(source, &target).with_context(|| format!("{}", source.display()))?;
        return Ok(target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default());
    }
    let (dir, url) = project_config.images(project_path);
    let dir = Path::new(project_path).join(dir);
    std::fs::create_dir_all(&dir)?;
    let target = free_path(&dir, &stem, &extension);
    std::fs::copy(source, &target).with_context(|| format!("{}", source.display()))?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(format!("{}/{name}", url.trim_end_matches('/')))
}

/// A draft of `content` in another language, beside the posts of that language under the same
/// file name, so the two are paired. Refused when that language already has the post.
#[allow(clippy::too_many_arguments)]
pub async fn translate_post(
    project_path: &str,
    project_config: &ProjectConfig,
    content: &str,
    filename: &str,
    from: &str,
    to: &str,
    provider: &LlmProvider,
    api_key: &str,
) -> Result<PathBuf> {
    let (queue, blog) = project_config.resolve_paths(project_path, to);
    let filename = &project_config.counterpart(filename, from, to);
    let target = queue.join(filename);
    if target.exists() || blog.join(filename).exists() {
        anyhow::bail!("{} already has {filename}", language_name(to));
    }

    let fields = frontmatter::fields(content);
    let key = description_key(content);
    let title = frontmatter::text(&fields, "title").unwrap_or_default();
    let description = frontmatter::text(&fields, key).unwrap_or_default();
    let (from_name, to_name) = (language_name(from), language_name(to));

    let system = format!(
        "You translate blog posts from {from_name} into {to_name}, as a native {to_name} writer would \
        put them: natural, not word for word, with the same tone, structure and markdown. Keep code, \
        commands, URLs, image paths and product names as they are. {}",
        project_config.system_prompt
    );
    let body_prompt = format!(
        "Translate this markdown into {to_name}. Output ONLY the translated markdown.\n\n{}",
        frontmatter::body(content)
    );
    let meta_prompt = format!(
        "Translate this blog post's title and description into {to_name}. Reply with ONLY a JSON object \
        (no markdown fences): {{\"title\": \"...\", \"description\": \"...\"}}\n\n\
        Title: {title}\nDescription: {description}"
    );
    let ask = |user: String| {
        let system = system.clone();
        async move { call(provider, &system, &user, api_key).await }
    };
    let (body, meta) = tokio::join!(ask(body_prompt), ask(meta_prompt));
    let body = body?;
    let meta = meta?;
    let meta = meta
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```");
    let meta: serde_json::Value = serde_json::from_str(meta.trim())
        .context("the model's title and description weren't JSON")?;

    let format = frontmatter::split(content).map_or(project_config.front_matter, |p| p.format);
    let text = |s: &str| Field::Text(s.to_string());
    let mut out = content.to_string();
    if let Some(t) = meta["title"].as_str() {
        out = frontmatter::set(&out, "title", &text(t), format);
    }
    if let Some(d) = meta["description"]
        .as_str()
        .filter(|_| fields.contains_key(key))
    {
        out = frontmatter::set(&out, key, &text(d), format);
    }
    for lang_key in ["lang", "locale", "language"] {
        if fields.contains_key(lang_key) {
            out = frontmatter::set(&out, lang_key, &text(to), format);
        }
    }
    out = frontmatter::set(
        &out,
        "date",
        &text(&today_like(
            &frontmatter::text(&fields, "date").unwrap_or_default(),
        )),
        format,
    );
    match project_config.drafts {
        Drafts::Folder => out = frontmatter::set(&out, "status", &text("draft"), format),
        Drafts::Flag => out = frontmatter::set(&out, "draft", &Field::Bool(true), format),
    }
    let out = frontmatter::with_body(&out, &body);

    std::fs::create_dir_all(target.parent().unwrap_or(&queue))?;
    std::fs::write(&target, out)?;
    Ok(target)
}

/// `node`, or on Windows the per-user copy that setup-windows.ps1 unpacks, when this process
/// started before that copy was added to PATH (e.g. launched from the installer's last page).
#[cfg(windows)]
pub(super) fn node_command() -> std::path::PathBuf {
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let user_node = std::path::Path::new(&local).join(r"Programs\nodejs\node.exe");
        let on_path = std::process::Command::new("node")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok();
        if !on_path && user_node.exists() {
            return user_node;
        }
    }
    "node".into()
}

/// Deletes a draft and its teasers. A published post is refused: with `draft: true` drafts,
/// both sit in the same folder.
pub fn delete_queued(
    project_path: &str,
    lang: &str,
    filename: &str,
    project_config: &ProjectConfig,
) -> Result<()> {
    let (queue_path, _) = project_config.resolve_paths(project_path, lang);
    let path = queue_path.join(filename);
    if project_config.drafts == Drafts::Flag {
        let content = std::fs::read_to_string(&path)?;
        if frontmatter::flag(&frontmatter::fields(&content), "draft") != Some(true) {
            anyhow::bail!("{filename} is published, not a draft");
        }
    }
    match bundle_folder(&queue_path, filename) {
        // The bundle's folder holds this post and what belongs to it — unless it holds the
        // post in other languages too, which stay with the images.
        Some(folder) if bundle_posts(&folder) <= 1 => std::fs::remove_dir_all(&folder)?,
        Some(_) => std::fs::remove_file(&path)?,
        None => {
            std::fs::remove_file(&path)?;
            let _ = std::fs::remove_file(social_path(&path)); // ignore if absent
        }
    }
    crate::plan::forget(project_path, lang, filename);
    Ok(())
}

pub(super) fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'á' | 'ã' | 'å' => 'a',
            'í' | 'ì' => 'i',
            'ó' | 'ò' | 'õ' | 'ø' => 'o',
            'ú' => 'u',
            'ý' | 'ÿ' => 'y',
            ' ' | '_' | '/' | '.' => '-',
            c => c,
        })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

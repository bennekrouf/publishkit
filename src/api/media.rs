//! A LinkedIn or X post's images or video: files beside the post, listed in its front matter
//! (`media: [cover.png]`), moved and deleted with it.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    Image,
    Video,
}

impl Media {
    /// By the file's extension; `None` for a file the networks don't take.
    pub fn of(name: &str) -> Option<Media> {
        let extension = Path::new(name).extension()?.to_str()?.to_lowercase();
        match extension.as_str() {
            "png" | "jpg" | "jpeg" | "gif" => Some(Media::Image),
            "mp4" | "mov" | "webm" | "mkv" | "avi" => Some(Media::Video),
            _ => None,
        }
    }
}

/// The media a post's file lists, by name in the post's folder.
pub fn media_of(content: &str) -> Vec<String> {
    frontmatter::list(&frontmatter::fields(content), "media")
}

/// Refused unless it is images (as many as the network takes) or one video.
fn check(names: &[String], channel: Channel) -> Result<()> {
    let mut kinds = Vec::with_capacity(names.len());
    for name in names {
        match Media::of(name) {
            Some(kind) => kinds.push(kind),
            None => bail!(
                "{name} is not an image (png, jpg, gif) or a video (mp4, mov, webm, mkv, avi)"
            ),
        }
    }
    if kinds.contains(&Media::Video) && kinds.len() > 1 {
        bail!(
            "a post on {} has images or one video, not both or several videos",
            channel.label()
        );
    }
    if kinds.len() > channel.max_images() {
        bail!(
            "{} takes at most {} images in a post",
            channel.label(),
            channel.max_images()
        );
    }
    Ok(())
}

/// Sets a post's media to `sources`, in that order: each a file to copy in beside the post,
/// under a free name, or the name of one it has already. The ones it had and no longer
/// lists are deleted; an empty list takes them all off. Gives the names it lists now.
pub fn set_media(post: &Path, channel: Channel, sources: &[PathBuf]) -> Result<Vec<String>> {
    let folder = post.parent().context("a post is in a folder")?;
    let content = std::fs::read_to_string(post).with_context(|| format!("{}", post.display()))?;
    let before = media_of(&content);
    let kept = |s: &Path| {
        s.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| {
                before.contains(n) && (s.parent() == Some(folder) || s.components().count() == 1)
            })
    };
    let names: Vec<String> = sources
        .iter()
        .map(|s| {
            kept(s).unwrap_or_else(|| {
                s.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
        })
        .collect();
    check(&names, channel)?;
    for source in sources {
        if kept(source).is_none() && !source.is_file() {
            bail!("{} is not a file", source.display());
        }
    }

    let mut now = Vec::with_capacity(sources.len());
    for source in sources {
        if let Some(name) = kept(source) {
            now.push(name);
            continue;
        }
        let target = free_media_path(folder, source);
        std::fs::copy(source, &target).with_context(|| format!("{}", source.display()))?;
        now.push(file_name(&target));
    }
    for gone in before.iter().filter(|n| !now.contains(n)) {
        let _ = std::fs::remove_file(folder.join(gone)); // already gone: nothing to do
    }
    write_list(post, &content, &now)?;
    Ok(now)
}

/// Moves a post's media from its folder to `to`, renamed where a name is taken there, and
/// gives the front matter listing their new names.
pub(super) fn move_media(content: &str, from: &Path, to: &Path) -> Result<String> {
    let names = media_of(content);
    if names.is_empty() {
        return Ok(content.to_string());
    }
    let mut moved = Vec::with_capacity(names.len());
    for name in &names {
        let source = from.join(name);
        if !source.is_file() {
            // Listed but missing: listed still, so the preview says it is missing.
            moved.push(name.clone());
            continue;
        }
        let target = free_media_path(to, &source);
        std::fs::rename(&source, &target)
            .or_else(|_| {
                std::fs::copy(&source, &target).and_then(|_| std::fs::remove_file(&source))
            })
            .with_context(|| format!("{}", source.display()))?;
        moved.push(file_name(&target));
    }
    Ok(frontmatter::set(
        content,
        "media",
        &Field::List(moved),
        Format::Yaml,
    ))
}

/// Deletes the media a post lists.
pub(super) fn delete_media(post: &Path) {
    let Some(folder) = post.parent() else { return };
    let content = std::fs::read_to_string(post).unwrap_or_default();
    for name in media_of(&content) {
        let _ = std::fs::remove_file(folder.join(name)); // already gone: nothing to do
    }
}

fn write_list(post: &Path, content: &str, names: &[String]) -> Result<()> {
    let updated = if names.is_empty() {
        frontmatter::remove(content, "media")
    } else {
        frontmatter::set(content, "media", &Field::List(names.to_vec()), Format::Yaml)
    };
    std::fs::write(post, updated).with_context(|| format!("{}", post.display()))?;
    Ok(())
}

/// `folder/cover.png`, or `cover-2.png` and so on when that is taken; never a post's name.
fn free_media_path(folder: &Path, source: &Path) -> PathBuf {
    let extension = source
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = source
        .file_stem()
        .map(|s| slugify(&s.to_string_lossy()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "media".into());
    free_path(folder, &stem, &extension)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

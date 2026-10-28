//! Working out what kind of site a folder holds, and where its posts are.

use super::*;

/// Which framework the site in this folder is built with, from the files it has.
pub fn detect_preset(project_path: &str) -> Option<Preset> {
    let root = Path::new(project_path);
    let has = |names: &[&str]| names.iter().any(|n| root.join(n).exists());
    let publishkit = detect_publishkit_layout(root);
    // A site PublishKit already publishes to, whatever it is built with.
    if root.join(PUBLISHKIT_INDEX_SCRIPT).is_file()
        || publishkit
            .langs
            .values()
            .any(|p| root.join(&p.queue).is_dir())
    {
        return Some(Preset::PublishKit);
    }
    let config_toml = std::fs::read_to_string(root.join("config.toml")).unwrap_or_default();
    let preset = if has(&["zola.toml"]) || config_toml.contains("base_url") {
        Some(Preset::Zola)
    } else if has(&["hugo.toml", "hugo.yaml", "hugo.yml", "hugo.json"])
        || (root.join("content").is_dir() && has(&["config.toml", "config.yaml", "archetypes"]))
    {
        Some(Preset::Hugo)
    } else if has(&["_config.yml", "_config.yaml", "_posts"]) {
        Some(Preset::Jekyll)
    } else if has(&[
        "astro.config.mjs",
        "astro.config.js",
        "astro.config.ts",
        "astro.config.mts",
    ]) {
        Some(Preset::Astro)
    } else if has(&[
        "docusaurus.config.js",
        "docusaurus.config.ts",
        "docusaurus.config.mjs",
    ]) {
        Some(Preset::Docusaurus)
    } else if has(&[
        ".eleventy.js",
        ".eleventy.cjs",
        "eleventy.config.js",
        "eleventy.config.mjs",
        "eleventy.config.cjs",
    ]) {
        Some(Preset::Eleventy)
    } else if has(&["next.config.js", "next.config.mjs", "next.config.ts"]) {
        Some(Preset::Next)
    } else {
        None
    };
    if preset.is_some() {
        return preset;
    }
    if !publishkit.langs.is_empty() {
        return Some(Preset::PublishKit);
    }
    let generic = spec(Preset::Generic);
    generic
        .dirs
        .iter()
        .any(|d| dir_has_posts(&root.join(d)))
        .then_some(Preset::Generic)
}

pub(super) fn spec(preset: Preset) -> Spec {
    let (dirs, drafts_dir, front_matter, post_url): (&[&str], _, _, _) = match preset {
        Preset::Hugo => (
            &["content/posts", "content/post", "content/blog"],
            None,
            Format::Toml,
            "{site_url}/posts/{slug}/",
        ),
        Preset::Zola => (
            &["content/blog", "content/posts"],
            None,
            Format::Toml,
            "{site_url}/blog/{slug}/",
        ),
        Preset::Jekyll => (
            &["_posts"],
            Some("_drafts"),
            Format::Yaml,
            "{site_url}/{year}/{month}/{day}/{slug}.html",
        ),
        Preset::Astro => (
            &["src/content/blog", "src/content/posts", "src/data/blog"],
            None,
            Format::Yaml,
            "{site_url}/blog/{slug}/",
        ),
        Preset::Eleventy => (
            &["posts", "blog", "src/posts", "src/blog", "content/blog"],
            None,
            Format::Yaml,
            "{site_url}/blog/{slug}/",
        ),
        Preset::Docusaurus => (&["blog"], None, Format::Yaml, "{site_url}/blog/{slug}"),
        Preset::Next => (
            &[
                "content/posts",
                "content/blog",
                "posts",
                "_posts",
                "src/content/blog",
                "blog",
            ],
            None,
            Format::Yaml,
            "{site_url}/blog/{slug}",
        ),
        Preset::Generic | Preset::PublishKit => (
            &["posts", "_posts", "blog", "articles", "content"],
            None,
            Format::Yaml,
            "{site_url}/blog/{slug}",
        ),
    };
    Spec {
        dirs,
        drafts_dir,
        front_matter,
        post_url,
    }
}

/// The posts folder of each language, for a framework's layout.
pub(super) fn find_posts_dirs(root: &Path, spec: &Spec) -> BTreeMap<String, LangPaths> {
    let mut langs = BTreeMap::new();
    let paths = |blog: String| LangPaths {
        queue: spec
            .drafts_dir
            .map(String::from)
            .unwrap_or_else(|| blog.clone()),
        blog,
        suffix: String::new(),
    };
    for dir in spec.dirs {
        // `content/en/posts` beside `content/fr/posts`: a multilingual site.
        if let Some((parent, leaf)) = dir.rsplit_once('/') {
            for lang in lang_dirs(&root.join(parent)) {
                if root.join(parent).join(&lang).join(leaf).is_dir() {
                    langs.insert(lang.clone(), paths(format!("{parent}/{lang}/{leaf}")));
                }
            }
        }
        if !langs.is_empty() {
            return langs;
        }
        if root.join(dir).is_dir() {
            // `post.md` beside `post.fr.md`: languages told apart by the file name.
            let marked = file_languages(&root.join(dir));
            let default = default_language(root)
                .or_else(|| site_language(&root.join(dir)))
                .unwrap_or_else(|| "en".into());
            langs.insert(default.clone(), paths(dir.to_string()));
            for lang in marked.into_iter().filter(|l| *l != default) {
                langs.insert(
                    lang.clone(),
                    LangPaths {
                        suffix: lang,
                        ..paths(dir.to_string())
                    },
                );
            }
            return langs;
        }
    }
    langs.insert("en".into(), paths(spec.dirs[0].to_string()));
    langs
}

/// The languages files in a folder are marked with: `fr` for `post.fr.md` or a bundle's
/// `index.fr.md`.
pub(super) fn file_languages(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = post_files(dir)
        .iter()
        .filter_map(|p| {
            p.file_name()?
                .to_str()
                .and_then(marked_language)
                .map(String::from)
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The language a site's files carry no mark for: Hugo's `defaultContentLanguage`, Zola's
/// `default_language`.
pub(super) fn default_language(root: &Path) -> Option<String> {
    for file in [
        "hugo.toml",
        "config.toml",
        "zola.toml",
        "hugo.yaml",
        "hugo.yml",
        "config.yaml",
    ] {
        let Ok(text) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            for key in ["defaultContentLanguage", "default_language"] {
                if let Some(rest) = line.strip_prefix(key) {
                    let value = rest
                        .trim_start()
                        .trim_start_matches(['=', ':'])
                        .trim()
                        .trim_matches(['"', '\'']);
                    if is_lang_code(&value.to_lowercase()) {
                        return Some(value.to_lowercase());
                    }
                }
            }
        }
    }
    None
}

pub(super) fn lang_dirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut langs: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_lang_code(n))
        .collect();
    langs.sort();
    langs
}

/// The PublishKit layout: `content/{lang}/{queue|draft|drafts}` and
/// `content/{lang}/{blog|published|posts}`.
pub(super) fn detect_publishkit_layout(root: &Path) -> ContentPaths {
    let mut langs = BTreeMap::new();
    for lang in lang_dirs(&root.join("content")) {
        let lang_dir = root.join("content").join(&lang);
        let queue = ["queue", "draft", "drafts"]
            .into_iter()
            .find(|n| lang_dir.join(n).is_dir());
        let blog = ["blog", "published", "posts"]
            .into_iter()
            .find(|n| dir_has_posts(&lang_dir.join(n)));
        if queue.is_some() || blog.is_some() {
            let base = format!("content/{lang}");
            langs.insert(
                lang,
                LangPaths {
                    queue: format!("{base}/{}", queue.unwrap_or("queue")),
                    blog: format!("{base}/{}", blog.unwrap_or("blog")),
                    suffix: String::new(),
                },
            );
        }
    }
    ContentPaths { langs }
}

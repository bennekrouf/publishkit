use crate::frontmatter::{self, Format};
use crate::plan::{Plan, Stage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SocialPosts {
    pub linkedin: String,
    pub twitter: String,
}

/// A standalone LinkedIn post about a product/feature (not tied to a blog article).
#[derive(Clone, Debug, PartialEq)]
pub struct LinkedInPost {
    pub path: PathBuf,
    pub filename: String,
    pub topic: String,
    pub lang: String,
    pub status: String,
    pub date: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    /// Where the file is: saving writes back here, whichever folder that is.
    pub path: PathBuf,
    pub filename: String,
    pub title: String,
    pub description: String,
    pub date: String,
    /// The language whose folder the post is in.
    pub lang: String,
    /// `draft` while it waits to be published, `published` once it is.
    pub status: String,
    pub tags: Vec<String>,
    pub body: String,
    pub full_content: String,
    pub social: Option<SocialPosts>,
    /// Where a draft is in the writing, from the site's plan.
    pub stage: Stage,
    /// When a draft is due out, from the site's plan: `2026-11-02T09:00`.
    pub publish_at: Option<String>,
}

impl Post {
    pub fn is_draft(&self) -> bool {
        self.status == "draft"
    }
}

/// One language's posts of one kind: drafts waiting, and what is out.
#[derive(Clone, Debug, PartialEq)]
pub struct Shelf<T> {
    pub queue: Vec<T>,
    pub published: Vec<T>,
}

impl<T> Default for Shelf<T> {
    fn default() -> Self {
        Self { queue: vec![], published: vec![] }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectPosts {
    /// Every language of the site, even one with no post yet.
    pub languages: Vec<String>,
    pub blog: BTreeMap<String, Shelf<Post>>,
    pub linkedin: BTreeMap<String, Shelf<LinkedInPost>>,
}

impl ProjectPosts {
    pub fn empty() -> Self {
        Self { languages: vec![], blog: BTreeMap::new(), linkedin: BTreeMap::new() }
    }

    pub fn blog(&self, lang: &str) -> Shelf<Post> {
        self.blog.get(lang).cloned().unwrap_or_default()
    }

    pub fn linkedin(&self, lang: &str) -> Shelf<LinkedInPost> {
        self.linkedin.get(lang).cloned().unwrap_or_default()
    }
}

/// The name of a language, for the model's instructions; the code itself when unknown.
pub fn language_name(code: &str) -> &str {
    let base = code.split(['-', '_']).next().unwrap_or(code);
    match base.to_ascii_lowercase().as_str() {
        "en" => "English", "fr" => "French", "de" => "German", "es" => "Spanish",
        "it" => "Italian", "pt" => "Portuguese", "nl" => "Dutch", "sv" => "Swedish",
        "da" => "Danish", "no" | "nb" => "Norwegian", "fi" => "Finnish", "pl" => "Polish",
        "cs" => "Czech", "ro" => "Romanian", "hu" => "Hungarian", "el" => "Greek",
        "tr" => "Turkish", "ru" => "Russian", "uk" => "Ukrainian", "ar" => "Arabic",
        "he" => "Hebrew", "fa" => "Persian", "hi" => "Hindi", "ja" => "Japanese",
        "ko" => "Korean", "zh" => "Chinese", "id" => "Indonesian", "vi" => "Vietnamese",
        "th" => "Thai",
        _ => code,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LlmProvider {
    /// api0 in one of its AI regions, on the shared licence's credits.
    Api0 { region: String },
}

impl Default for LlmProvider {
    fn default() -> Self {
        LlmProvider::api0(crate::api0::REGIONS[0].id)
    }
}

impl LlmProvider {
    pub fn label(&self) -> &'static str {
        self.region().map_or("api0", |r| r.display)
    }
    pub fn model(&self) -> &str {
        match self {
            LlmProvider::Api0 { region } => region,
        }
    }
    pub fn api_url(&self) -> String {
        format!("{}/chat/completions", crate::api0::BASE_URL)
    }
    /// One click in Settings: api0, in this region.
    pub fn api0(region: &str) -> Self {
        LlmProvider::Api0 { region: region.to_string() }
    }
    /// The api0 region this is set to, if api0 still offers it.
    pub fn region(&self) -> Option<&'static crate::api0::Region> {
        match self {
            LlmProvider::Api0 { region } => crate::api0::region(region),
        }
    }
}

/// A provider saved before api0 was the only one reads as the default region, so the rest of
/// the config (sites, accounts) is kept.
fn provider_or_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<LlmProvider, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        Known(LlmProvider),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Saved::deserialize(d)? {
        Saved::Known(p) => p,
        Saved::Other(_) => LlmProvider::default(),
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub project_path: Option<String>,
    #[serde(default, deserialize_with = "provider_or_default")]
    pub provider: LlmProvider,
    /// Accounts on the platforms posts are copied to.
    #[serde(default)]
    pub crosspost: crate::crosspost::Accounts,
    /// Sites opened before, the latest first: the switcher's list.
    #[serde(default)]
    pub recent_projects: Vec<String>,
    /// The post list's width, as last dragged; `None`: the default.
    #[serde(default)]
    pub left_panel_width: Option<f64>,
}

/// Sites the switcher remembers.
const RECENT_SITES: usize = 12;

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            project_path: None, provider: LlmProvider::default(),
            crosspost: Default::default(), recent_projects: vec![], left_panel_width: None,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        // Still the old name: the API keys are saved here, and the last rename
        // (Blog Manager → Blog Toolkit) lost them by moving it.
        directories::ProjectDirs::from("com", "mayorana", "blog-toolkit")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        if let Ok(json) = serde_json::to_string_pretty(self) { let _ = std::fs::write(path, json); }
    }

    /// Makes `path` the open site, first in the list of sites.
    pub fn open_project(&mut self, path: &str) {
        // The site open until now, which an older config never listed.
        if let Some(previous) = self.project_path.clone() {
            if !self.recent_projects.contains(&previous) {
                self.recent_projects.insert(0, previous);
            }
        }
        self.recent_projects.retain(|p| p != path);
        self.recent_projects.insert(0, path.to_string());
        self.recent_projects.truncate(RECENT_SITES);
        self.project_path = Some(path.to_string());
    }

    /// Drops `path` from the list; when it was open, the next site opens, or none.
    pub fn forget_project(&mut self, path: &str) {
        self.recent_projects.retain(|p| p != path);
        if self.project_path.as_deref() == Some(path) {
            self.project_path = None;
            self.project_path = self.sites().into_iter().next();
        }
    }

    /// The sites to switch between, the open one first; folders that are gone are left out.
    pub fn sites(&self) -> Vec<String> {
        let mut sites: Vec<String> = self.project_path.iter().cloned().collect();
        for p in &self.recent_projects {
            if !sites.contains(p) {
                sites.push(p.clone());
            }
        }
        sites.retain(|p| Path::new(p).is_dir());
        sites
    }

    /// What to say when [`Self::resolved_api_key`] is empty.
    pub fn missing_key(&self) -> String {
        "api0 needs a licence: paste one in any Mayorana app, or set API0_API_KEY".to_string()
    }

    /// The shared licence api0 is paid with.
    pub fn resolved_api_key(&self) -> String {
        crate::api0::licence().unwrap_or_default()
    }
}

/// Detected content directory layout per language.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LangPaths {
    pub queue: String,
    pub blog: String,
    /// Where languages share a folder and the file name says which (`post.fr.md`, Hugo's and
    /// Zola's way): this language's mark. Empty for the default language, whose files have none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suffix: String,
}

/// Mapping of language codes to their content paths (relative to project root).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ContentPaths {
    pub langs: BTreeMap<String, LangPaths>,
}

/// The framework a site is built with. It sets where posts are, how a draft is told apart,
/// the front matter of a new post, and its address on the site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    /// `content/{lang}/queue` for drafts, `content/{lang}/blog` once published.
    PublishKit,
    Hugo,
    Jekyll,
    Astro,
    Eleventy,
    Zola,
    Docusaurus,
    Next,
    /// A folder of markdown files, drafts marked `draft: true`.
    Generic,
}

impl Preset {
    pub const ALL: [Preset; 9] = [
        Preset::PublishKit, Preset::Hugo, Preset::Jekyll, Preset::Astro, Preset::Eleventy,
        Preset::Zola, Preset::Docusaurus, Preset::Next, Preset::Generic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::PublishKit => "PublishKit folders",
            Preset::Hugo => "Hugo",
            Preset::Jekyll => "Jekyll",
            Preset::Astro => "Astro",
            Preset::Eleventy => "Eleventy",
            Preset::Zola => "Zola",
            Preset::Docusaurus => "Docusaurus",
            Preset::Next => "Next.js",
            Preset::Generic => "Markdown folder",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Preset::PublishKit => "publishkit",
            Preset::Hugo => "hugo",
            Preset::Jekyll => "jekyll",
            Preset::Astro => "astro",
            Preset::Eleventy => "eleventy",
            Preset::Zola => "zola",
            Preset::Docusaurus => "docusaurus",
            Preset::Next => "next",
            Preset::Generic => "generic",
        }
    }

    pub fn from_key(key: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// How a draft is told apart from a published post.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Drafts {
    /// Drafts have a folder of their own, and publishing moves them into the blog's.
    #[default]
    Folder,
    /// Drafts sit with the published posts, marked `draft: true`.
    Flag,
}

pub const DEFAULT_SYSTEM_PROMPT: &str = "You write clear, engaging, well-structured blog posts. \
    Use concrete examples, short paragraphs and descriptive section headings. \
    Avoid jargon, filler and clichés.";

/// Per-project config loaded from `publishkit.yaml` at the project root.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectConfig {
    pub site_name: String,
    pub site_url: String,
    pub author: String,
    pub default_tags: Vec<String>,
    pub system_prompt: String,
    pub social_enabled: bool,
    pub content_paths: Option<ContentPaths>,
    /// Absent in a file written before presets: that site keeps the folders it was using.
    pub preset: Option<Preset>,
    pub drafts: Drafts,
    /// The front matter of a new post.
    pub front_matter: Format,
    /// `md` or `mdx`, for a new post.
    pub extension: String,
    /// A new post is a page bundle, `my-post/index.md`, a folder to keep its images in.
    pub bundles: bool,
    /// A new post's body starts with its title as a `#` heading (for themes that don't print it).
    pub title_heading: bool,
    /// Publishing renames `my-post.md` to `2026-01-02-my-post.md`, as Jekyll wants.
    pub date_in_filename: bool,
    /// Run in the site's folder after a post is published, e.g. to rebuild an index. Empty: none.
    pub publish_command: String,
    /// A post's address, for its social teasers: `{site_url}`, `{lang}`, `{slug}`, `{year}`,
    /// `{month}` and `{day}` are filled in.
    pub post_url: String,
    /// Where LinkedIn posts are kept, per language; empty: `content/{lang}/linkedin`.
    pub linkedin_dir: String,
    /// Puts the site online, run in its folder from the Deploy button: `git push`, `npm run
    /// deploy`, `vercel --prod`… Empty: no Deploy.
    pub deploy_command: String,
    /// Deploy after each publish too, the scheduled ones included.
    pub deploy_after_publish: bool,
    /// Where images added to a post are copied, in the site's folder; empty: the preset's.
    pub images_dir: String,
    /// The address the site serves `images_dir` at, for the markdown link; empty: the preset's.
    pub images_url: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            site_name: String::new(),
            site_url: String::new(),
            author: String::new(),
            default_tags: vec![],
            social_enabled: true,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            content_paths: None,
            preset: None,
            drafts: Drafts::Folder,
            front_matter: Format::Yaml,
            extension: "md".to_string(),
            bundles: false,
            title_heading: true,
            date_in_filename: false,
            publish_command: String::new(),
            post_url: String::new(),
            linkedin_dir: String::new(),
            images_dir: String::new(),
            images_url: String::new(),
            deploy_command: String::new(),
            deploy_after_publish: false,
        }
    }
}

/// A site's name for the switcher: its configured name, or its folder's. Read without
/// detecting anything, so listing sites never writes to them.
pub fn site_label(project_path: &str) -> String {
    let named = std::fs::read_to_string(config_path(project_path))
        .ok()
        .and_then(|s| serde_yaml::from_str::<serde_yaml::Value>(&s).ok())
        .and_then(|v| v.get("site_name").and_then(|n| n.as_str()).map(|n| n.trim().to_string()))
        .filter(|n| !n.is_empty());
    named.unwrap_or_else(|| {
        Path::new(project_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| project_path.to_string())
    })
}

/// The project's config file: `publishkit.yaml`, or `blog-toolkit.yaml` in a
/// project set up before the app was renamed PublishKit. That file is kept
/// rather than renamed: it sits in the person's own repository.
fn config_path(project_path: &str) -> PathBuf {
    let path = PathBuf::from(project_path).join("publishkit.yaml");
    let legacy = PathBuf::from(project_path).join("blog-toolkit.yaml");
    if !path.exists() && legacy.exists() {
        legacy
    } else {
        path
    }
}

const PUBLISHKIT_INDEX_SCRIPT: &str = "scripts/generate-blog-data.js";

impl ProjectConfig {
    /// Defaults for a site PublishKit hasn't seen: named after its folder.
    fn neutral(project_path: &str) -> Self {
        let site_name = Path::new(project_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self { site_name, ..Self::default() }
    }

    pub fn load(project_path: &str) -> Self {
        let path = config_path(project_path);
        let file = std::fs::read_to_string(&path).ok();
        let (mut config, writable) = match file.as_deref().map(serde_yaml::from_str::<Self>) {
            Some(Ok(config)) => (config, true),
            // Never written over: a mistake in it is the person's to fix, not ours to erase.
            Some(Err(_)) => (Self::neutral(project_path), false),
            None => (Self::neutral(project_path), true),
        };
        if config.preset.is_none() {
            let legacy = file.is_some() && writable && config.content_paths.is_some();
            let preset = if legacy { Some(Preset::PublishKit) } else { detect_preset(project_path) };
            if let Some(preset) = preset {
                config.apply_preset(preset, project_path, legacy);
                // Persist detection to yaml so it only runs once
                if writable {
                    config.save(project_path);
                }
            }
        }
        config
    }

    pub fn save(&self, project_path: &str) {
        let path = config_path(project_path);
        if let Ok(yaml) = serde_yaml::to_string(self) {
            let _ = std::fs::write(path, yaml);
        }
    }

    /// Sets the folders and conventions of `preset`, found in the site's folder. `keep_paths`
    /// leaves the folders as they are configured.
    pub fn apply_preset(&mut self, preset: Preset, project_path: &str, keep_paths: bool) {
        let root = Path::new(project_path);
        self.preset = Some(preset);
        self.linkedin_dir = String::new();
        self.date_in_filename = false;

        if preset == Preset::PublishKit {
            if !keep_paths {
                let found = detect_publishkit_layout(root);
                self.content_paths = Some(if found.langs.is_empty() {
                    let lang = "en".to_string();
                    let paths = LangPaths { queue: "content/en/queue".into(), blog: "content/en/blog".into(), suffix: String::new() };
                    ContentPaths { langs: BTreeMap::from([(lang, paths)]) }
                } else {
                    found
                });
            }
            self.drafts = Drafts::Folder;
            self.front_matter = Format::Yaml;
            self.extension = "md".into();
            self.title_heading = true;
            self.publish_command = if root.join(PUBLISHKIT_INDEX_SCRIPT).is_file() {
                format!("node {PUBLISHKIT_INDEX_SCRIPT}")
            } else {
                String::new()
            };
            self.post_url = "{site_url}/{lang}/blog/{slug}".into();
            return;
        }

        let spec = spec(preset);
        self.drafts = if spec.drafts_dir.is_some() { Drafts::Folder } else { Drafts::Flag };
        self.title_heading = false;
        self.date_in_filename = preset == Preset::Jekyll;
        self.publish_command = String::new();
        self.post_url = spec.post_url.into();
        // Kept out of the site's own content, where a framework would publish them as pages.
        self.linkedin_dir = ".publishkit/linkedin".into();
        if !keep_paths {
            let langs = find_posts_dirs(root, &spec);
            self.content_paths = Some(ContentPaths { langs });
        }
        let blog = self.languages().first().map(|l| self.resolve_paths(project_path, l).1);
        let sample = blog.as_deref().and_then(first_post);
        self.front_matter = match sample.as_deref().map(std::fs::read_to_string) {
            Some(Ok(c)) if c.starts_with("+++") => Format::Toml,
            Some(Ok(c)) if c.starts_with("---") => Format::Yaml,
            _ => spec.front_matter,
        };
        self.extension = match sample.as_deref().and_then(|p| p.extension()) {
            Some(e) if e == "mdx" => "mdx".into(),
            _ => "md".into(),
        };
        self.bundles = sample.as_deref().is_some_and(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(is_bundle_index));
    }

    /// Get resolved paths for a language, falling back to default layout.
    pub fn resolve_paths(&self, project_path: &str, lang: &str) -> (PathBuf, PathBuf) {
        if let Some(cp) = &self.content_paths {
            if let Some(lp) = cp.langs.get(lang) {
                return (
                    PathBuf::from(project_path).join(&lp.queue),
                    PathBuf::from(project_path).join(&lp.blog),
                );
            }
        }
        // Default fallback
        let root = PathBuf::from(project_path).join("content").join(lang);
        (root.join("queue"), root.join("blog"))
    }

    /// Paths for standalone LinkedIn posts: `{linkedin_dir}/{lang}/{queue,published}`, or
    /// content/{lang}/linkedin/{queue,published}.
    pub fn resolve_linkedin_paths(&self, project_path: &str, lang: &str) -> (PathBuf, PathBuf) {
        let root = if self.linkedin_dir.is_empty() {
            PathBuf::from(project_path).join("content").join(lang).join("linkedin")
        } else {
            PathBuf::from(project_path).join(&self.linkedin_dir).join(lang)
        };
        (root.join("queue"), root.join("published"))
    }

    /// This language's mark in file names (`fr` for `post.fr.md`); empty when it has none.
    pub fn language_mark(&self, lang: &str) -> &str {
        self.content_paths.as_ref().and_then(|c| c.langs.get(lang)).map_or("", |p| p.suffix.as_str())
    }

    /// Every language mark the site's file names use.
    pub fn language_marks(&self) -> Vec<String> {
        self.content_paths.iter().flat_map(|c| c.langs.values()).map(|p| p.suffix.clone()).filter(|s| !s.is_empty()).collect()
    }

    /// The file name of `filename`'s counterpart in another language: the same post, its
    /// mark changed (`post.md` ↔ `post.fr.md`); the same name where languages have folders.
    pub fn counterpart(&self, filename: &str, from: &str, to: &str) -> String {
        let (from, to) = (self.language_mark(from), self.language_mark(to));
        if from.is_empty() && to.is_empty() {
            return filename.to_string();
        }
        let (stem, extension) = filename.rsplit_once('.').unwrap_or((filename, "md"));
        let base = if from.is_empty() { stem } else { stem.strip_suffix(&format!(".{from}")).unwrap_or(stem) };
        if to.is_empty() { format!("{base}.{extension}") } else { format!("{base}.{to}.{extension}") }
    }

    /// The site's languages, in order; `en` when none is known yet.
    pub fn languages(&self) -> Vec<String> {
        if let Some(cp) = &self.content_paths {
            if !cp.langs.is_empty() {
                return cp.langs.keys().cloned().collect();
            }
        }
        vec!["en".to_string()]
    }

    /// Renames a language, keeping its folders: for a one-language site detected with the
    /// wrong code.
    pub fn rename_language(&mut self, from: &str, to: &str) {
        let to = to.trim().to_lowercase();
        if to.is_empty() || from == to {
            return;
        }
        if let Some(cp) = &mut self.content_paths {
            if let Some(paths) = cp.langs.remove(from) {
                cp.langs.insert(to, paths);
            }
        }
    }

    /// Where images go, and the address they are linked at: as configured, or where the
    /// site's framework serves static files from.
    pub fn images(&self, project_path: &str) -> (String, String) {
        if !self.images_dir.is_empty() {
            let url = if self.images_url.is_empty() {
                format!("/{}", self.images_dir.trim_matches('/'))
            } else {
                self.images_url.clone()
            };
            return (self.images_dir.clone(), url);
        }
        let public = Path::new(project_path).join("public").is_dir();
        let (dir, url) = match self.preset {
            Some(Preset::Hugo) | Some(Preset::Zola) => ("static/images", "/images"),
            Some(Preset::Jekyll) => ("assets/images", "/assets/images"),
            Some(Preset::Docusaurus) => ("static/img", "/img"),
            Some(Preset::Astro) | Some(Preset::Next) => ("public/images", "/images"),
            _ if public => ("public/images/blog", "/images/blog"),
            _ => ("images", "/images"),
        };
        (dir.to_string(), url.to_string())
    }

    /// Where a post lives on the site, by its `slug` field when it has one.
    pub fn address(&self, lang: &str, filename: &str, content: &str) -> String {
        match frontmatter::text(&frontmatter::fields(content), "slug") {
            Some(slug) if !slug.trim().is_empty() => self.post_url(lang, &format!("{}.md", slug.trim())),
            _ => self.post_url(lang, filename),
        }
    }

    /// Where a post lives on the site.
    pub fn post_url(&self, lang: &str, filename: &str) -> String {
        let pattern = if self.post_url.is_empty() { "{site_url}/{lang}/blog/{slug}" } else { &self.post_url };
        let today = chrono::Local::now();
        pattern
            .replace("{site_url}", self.site_url.trim_end_matches('/'))
            .replace("{lang}", lang)
            .replace("{slug}", &slug_of(filename))
            .replace("{year}", &today.format("%Y").to_string())
            .replace("{month}", &today.format("%m").to_string())
            .replace("{day}", &today.format("%d").to_string())
    }
}

/// A post's slug: its file name without the extension or a Jekyll date; a bundle's folder name.
pub fn slug_of(filename: &str) -> String {
    if let Some((folder, _)) = filename.split_once('/') {
        return folder.to_string();
    }
    let stem = Path::new(filename).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    // `post.fr` is the French `post`.
    let stem = match marked_language(filename) {
        Some(mark) => stem.strip_suffix(&format!(".{mark}")).unwrap_or(&stem).to_string(),
        None => stem,
    };
    match stem.get(..11) {
        Some(prefix) if is_date_prefix(prefix) => stem[11..].to_string(),
        _ => stem,
    }
}

/// `2026-01-02-`
pub fn is_date_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 11
        && b.iter().enumerate().all(|(i, c)| match i { 4 | 7 | 10 => *c == b'-', _ => c.is_ascii_digit() })
}

/// Every post file of the project with its modification time, sorted: when it differs from the
/// last one, a post was added, changed, moved or deleted, here or by `publishkit mcp`.
pub fn content_stamp(project_path: &str) -> Vec<(PathBuf, std::time::SystemTime)> {
    let config = ProjectConfig::load(project_path);
    let mut dirs = Vec::new();
    for lang in config.languages() {
        let (queue, blog) = config.resolve_paths(project_path, &lang);
        let (li_queue, li_published) = config.resolve_linkedin_paths(project_path, &lang);
        dirs.extend([queue, blog, li_queue, li_published]);
    }
    dirs.sort();
    dirs.dedup();
    let mut stamp = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
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
    let plan = Path::new(project_path).join(".publishkit").join("plan.json");
    if let Ok(modified) = std::fs::metadata(&plan).and_then(|m| m.modified()) {
        stamp.push((plan, modified));
    }
    stamp.sort();
    stamp
}

pub fn load_posts(project_path: &str) -> ProjectPosts {
    let config = ProjectConfig::load(project_path);
    let mut posts = ProjectPosts { languages: config.languages(), ..ProjectPosts::empty() };
    let plan = Plan::load(project_path);
    let marks = config.language_marks();
    for lang in &posts.languages {
        let (queue, blog) = config.resolve_paths(project_path, lang);
        let own = config.language_mark(lang);
        // Where languages share a folder, each keeps the files with its mark.
        let mine = |p: &Post| {
            let mark = p.path.file_name().and_then(|n| n.to_str()).and_then(marked_language).filter(|m| marks.iter().any(|x| x == m));
            mark.unwrap_or("") == own
        };
        let shelf = match config.drafts {
            Drafts::Folder => Shelf {
                queue: read_posts(&queue, lang, true, config.drafts),
                published: read_posts(&blog, lang, false, config.drafts),
            },
            Drafts::Flag => {
                let (queue, published) =
                    read_posts(&blog, lang, false, config.drafts).into_iter().partition(Post::is_draft);
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
        let (li_queue, li_published) = config.resolve_linkedin_paths(project_path, lang);
        posts.linkedin.insert(
            lang.clone(),
            Shelf { queue: read_linkedin_posts(&li_queue, lang), published: read_linkedin_posts(&li_published, lang) },
        );
    }
    posts
}

fn is_post_file(path: &Path) -> bool {
    matches!(path.extension().and_then(|x| x.to_str()), Some("md" | "mdx"))
}

/// The file of a page bundle, a folder holding a post beside its images: `index.md`, or
/// `index.fr.md` and so on where the file name marks the language.
pub fn is_bundle_index(name: &str) -> bool {
    name.starts_with("index.") && (name.ends_with(".md") || name.ends_with(".mdx"))
}

/// The posts of a bundle folder, one per language.
fn bundle_files(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else { return vec![] };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(is_bundle_index))
        .collect()
}

/// The posts in a folder: its markdown files, and the `index.md` of each bundle in it. An
/// `index.md` or `_index.md` of the folder itself is its list page, not a post.
pub fn post_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .flat_map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                bundle_files(&path)
            } else if is_post_file(&path) && !name.starts_with("index.") && !name.starts_with("_index.") {
                vec![path]
            } else {
                vec![]
            }
        })
        .collect();
    files.sort();
    files
}

/// A post's name in its folder: `my-post.md`, or `my-post/index.md` for a bundle.
fn post_name(dir: &Path, path: &Path) -> Option<String> {
    let name = path.strip_prefix(dir).ok()?.to_string_lossy().replace('\\', "/");
    Some(name)
}

fn read_linkedin_posts(dir: &Path, lang: &str) -> Vec<LinkedInPost> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut posts: Vec<LinkedInPost> = entries
        .filter_map(|e| e.ok())
        .filter(|e| is_post_file(&e.path()))
        .filter_map(|e| parse_linkedin_post(e.path(), lang))
        .collect();
    posts.sort_by(|a, b| b.date.cmp(&a.date));
    posts
}

fn parse_linkedin_post(path: PathBuf, lang: &str) -> Option<LinkedInPost> {
    let full_content = std::fs::read_to_string(&path).ok()?;
    let filename = path.file_name()?.to_string_lossy().to_string();
    let fields = frontmatter::fields(&full_content);

    let topic  = frontmatter::text(&fields, "topic").unwrap_or_else(|| filename.clone());
    let date   = frontmatter::text(&fields, "date").unwrap_or_default();
    let status = frontmatter::text(&fields, "status").unwrap_or_else(|| "draft".to_string());
    let body   = frontmatter::body(&full_content).to_string();

    Some(LinkedInPost { path, filename, topic, lang: lang.to_string(), status, date, body })
}

/// Which framework the site in this folder is built with, from the files it has.
pub fn detect_preset(project_path: &str) -> Option<Preset> {
    let root = Path::new(project_path);
    let has = |names: &[&str]| names.iter().any(|n| root.join(n).exists());
    let publishkit = detect_publishkit_layout(root);
    // A site PublishKit already publishes to, whatever it is built with.
    if root.join(PUBLISHKIT_INDEX_SCRIPT).is_file()
        || publishkit.langs.values().any(|p| root.join(&p.queue).is_dir())
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
    } else if has(&["astro.config.mjs", "astro.config.js", "astro.config.ts", "astro.config.mts"]) {
        Some(Preset::Astro)
    } else if has(&["docusaurus.config.js", "docusaurus.config.ts", "docusaurus.config.mjs"]) {
        Some(Preset::Docusaurus)
    } else if has(&[".eleventy.js", ".eleventy.cjs", "eleventy.config.js", "eleventy.config.mjs", "eleventy.config.cjs"]) {
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
    generic.dirs.iter().any(|d| dir_has_posts(&root.join(d))).then_some(Preset::Generic)
}

/// Where a framework keeps its posts.
struct Spec {
    /// Candidates, the first that exists wins; `content/{lang}/posts` is looked for too.
    dirs: &'static [&'static str],
    /// A folder of its own for drafts (Jekyll's `_drafts`); otherwise `draft: true`.
    drafts_dir: Option<&'static str>,
    front_matter: Format,
    post_url: &'static str,
}

fn spec(preset: Preset) -> Spec {
    let (dirs, drafts_dir, front_matter, post_url): (&[&str], _, _, _) = match preset {
        Preset::Hugo => (&["content/posts", "content/post", "content/blog"], None, Format::Toml, "{site_url}/posts/{slug}/"),
        Preset::Zola => (&["content/blog", "content/posts"], None, Format::Toml, "{site_url}/blog/{slug}/"),
        Preset::Jekyll => (&["_posts"], Some("_drafts"), Format::Yaml, "{site_url}/{year}/{month}/{day}/{slug}.html"),
        Preset::Astro => (&["src/content/blog", "src/content/posts", "src/data/blog"], None, Format::Yaml, "{site_url}/blog/{slug}/"),
        Preset::Eleventy => (&["posts", "blog", "src/posts", "src/blog", "content/blog"], None, Format::Yaml, "{site_url}/blog/{slug}/"),
        Preset::Docusaurus => (&["blog"], None, Format::Yaml, "{site_url}/blog/{slug}"),
        Preset::Next => (&["content/posts", "content/blog", "posts", "_posts", "src/content/blog", "blog"], None, Format::Yaml, "{site_url}/blog/{slug}"),
        Preset::Generic | Preset::PublishKit => (&["posts", "_posts", "blog", "articles", "content"], None, Format::Yaml, "{site_url}/blog/{slug}"),
    };
    Spec { dirs, drafts_dir, front_matter, post_url }
}

/// The posts folder of each language, for a framework's layout.
fn find_posts_dirs(root: &Path, spec: &Spec) -> BTreeMap<String, LangPaths> {
    let mut langs = BTreeMap::new();
    let paths = |blog: String| LangPaths {
        queue: spec.drafts_dir.map(String::from).unwrap_or_else(|| blog.clone()),
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
                langs.insert(lang.clone(), LangPaths { suffix: lang, ..paths(dir.to_string()) });
            }
            return langs;
        }
    }
    langs.insert("en".into(), paths(spec.dirs[0].to_string()));
    langs
}

/// The languages files in a folder are marked with: `fr` for `post.fr.md` or a bundle's
/// `index.fr.md`.
fn file_languages(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = post_files(dir)
        .iter()
        .filter_map(|p| p.file_name()?.to_str().and_then(marked_language).map(String::from))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// `fr` for `post.fr.md`, when what sits before the extension looks like a language.
fn marked_language(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".md").or_else(|| name.strip_suffix(".mdx"))?;
    let (_, mark) = stem.rsplit_once('.')?;
    is_lang_code(mark).then_some(mark)
}

/// The language a site's files carry no mark for: Hugo's `defaultContentLanguage`, Zola's
/// `default_language`.
fn default_language(root: &Path) -> Option<String> {
    for file in ["hugo.toml", "config.toml", "zola.toml", "hugo.yaml", "hugo.yml", "config.yaml"] {
        let Ok(text) = std::fs::read_to_string(root.join(file)) else { continue };
        for line in text.lines() {
            let line = line.trim();
            for key in ["defaultContentLanguage", "default_language"] {
                if let Some(rest) = line.strip_prefix(key) {
                    let value = rest.trim_start().trim_start_matches(['=', ':']).trim().trim_matches(['"', '\'']);
                    if is_lang_code(&value.to_lowercase()) {
                        return Some(value.to_lowercase());
                    }
                }
            }
        }
    }
    None
}

/// A one-language site's language, from the `lang` of its first post.
fn site_language(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(first_post(dir)?).ok()?;
    let fields = frontmatter::fields(&content);
    let lang = frontmatter::text(&fields, "lang").or_else(|| frontmatter::text(&fields, "language"))?;
    is_lang_code(&lang.to_lowercase()).then(|| lang.to_lowercase())
}

/// `en`, `fra`, `pt-br`: a folder named after a language, not `blog` or `docs`.
fn is_lang_code(name: &str) -> bool {
    let (base, region) = name.split_once('-').map_or((name, None), |(b, r)| (b, Some(r)));
    (2..=3).contains(&base.len())
        && base.chars().all(|c| c.is_ascii_lowercase())
        && region.is_none_or(|r| (2..=4).contains(&r.len()) && r.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn lang_dirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
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
fn detect_publishkit_layout(root: &Path) -> ContentPaths {
    let mut langs = BTreeMap::new();
    for lang in lang_dirs(&root.join("content")) {
        let lang_dir = root.join("content").join(&lang);
        let queue = ["queue", "draft", "drafts"].into_iter().find(|n| lang_dir.join(n).is_dir());
        let blog = ["blog", "published", "posts"].into_iter().find(|n| dir_has_posts(&lang_dir.join(n)));
        if queue.is_some() || blog.is_some() {
            let base = format!("content/{lang}");
            langs.insert(lang, LangPaths {
                queue: format!("{base}/{}", queue.unwrap_or("queue")),
                blog: format!("{base}/{}", blog.unwrap_or("blog")),
                suffix: String::new(),
            });
        }
    }
    ContentPaths { langs }
}

/// The first post in a folder, by name.
fn first_post(dir: &Path) -> Option<PathBuf> {
    post_files(dir).into_iter().next()
}

fn dir_has_posts(dir: &Path) -> bool {
    first_post(dir)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .is_some_and(|c| frontmatter::split(&c).is_some())
}

fn read_posts(dir: &Path, lang: &str, in_queue: bool, drafts: Drafts) -> Vec<Post> {
    let mut posts: Vec<Post> = post_files(dir)
        .into_iter()
        .filter_map(|path| {
            let name = post_name(dir, &path)?;
            parse_post(path, name, lang, in_queue, drafts)
        })
        .collect();
    // Newest first; undated drafts on top.
    posts.sort_by(|a, b| {
        (a.date.is_empty(), &a.date).cmp(&(b.date.is_empty(), &b.date)).reverse().then(a.filename.cmp(&b.filename))
    });
    posts
}

fn parse_post(path: PathBuf, filename: String, lang: &str, in_queue: bool, drafts: Drafts) -> Option<Post> {
    let full_content = std::fs::read_to_string(&path).ok()?;
    let fields = frontmatter::fields(&full_content);
    let text = |key: &str| frontmatter::text(&fields, key);

    let title       = text("title").unwrap_or_else(|| slug_of(&filename));
    let description = text("description").or_else(|| text("summary")).or_else(|| text("excerpt")).unwrap_or_default();
    let date        = text("date").unwrap_or_default();
    let tags        = frontmatter::list(&fields, "tags");
    let marked      = text("status");
    let status = match drafts {
        Drafts::Folder => marked.unwrap_or_else(|| if in_queue { "draft" } else { "published" }.to_string()),
        Drafts::Flag if frontmatter::flag(&fields, "draft") == Some(true) || marked.as_deref() == Some("draft") => {
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
        path, filename, title, description, date, lang: lang.to_string(), status, tags, body, full_content, social,
        stage: Stage::Draft, publish_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_is_saved_and_read_back() {
        let mut cfg = AppConfig { provider: LlmProvider::api0("china"), ..AppConfig::default() };
        let back: AppConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.provider.region().map(|r| r.id), Some("china"));
        assert_eq!(back.provider.label(), "api0 · China");
        assert_eq!(back.provider.model(), "china");
        // A config saved with a provider of its own key still reads, on the default region.
        cfg = serde_json::from_str(r#"{"project_path":"/site","provider":"Other","api_key":"k"}"#).unwrap();
        assert_eq!(cfg.project_path.as_deref(), Some("/site"));
        assert_eq!(cfg.provider.region().map(|r| r.id), Some("europe"));
    }

    fn site(files: &[(&str, &str)]) -> PathBuf {
        // A folder of its own per test: tests run in parallel, and a clock can repeat.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("publishkit-test-{}-{n}", std::process::id()));
        for (path, content) in files {
            let p = root.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn a_hugo_site_is_found_with_its_drafts_flagged() {
        let root = site(&[
            ("hugo.toml", "baseURL = 'https://x.org'"),
            ("content/posts/a.md", "+++\ntitle = \"A\"\ndate = 2026-01-01\ndraft = true\n+++\nA"),
            ("content/posts/b.md", "+++\ntitle = \"B\"\ndate = 2026-01-02\n+++\nB"),
        ]);
        let project = root.to_string_lossy();
        assert_eq!(detect_preset(&project), Some(Preset::Hugo));
        let cfg = ProjectConfig::load(&project);
        assert_eq!(cfg.drafts, Drafts::Flag);
        assert_eq!(cfg.front_matter, Format::Toml);
        let posts = load_posts(&project);
        let shelf = posts.blog("en");
        assert_eq!(shelf.queue.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["A"]);
        assert_eq!(shelf.published.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["B"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_language_folder_is_a_language() {
        let root = site(&[
            ("content/de/queue/a.md", "---\ntitle: A\n---\n"),
            ("content/fr/blog/b.md", "---\ntitle: B\n---\n"),
            ("content/pt-br/blog/c.md", "---\ntitle: C\n---\n"),
            ("content/images/x.md", "---\ntitle: X\n---\n"),
        ]);
        let project = root.to_string_lossy();
        let posts = load_posts(&project);
        assert_eq!(posts.languages, ["de", "fr", "pt-br"]);
        assert_eq!(posts.blog("de").queue.len(), 1);
        assert_eq!(posts.blog("pt-br").published[0].status, "published");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_config_from_before_presets_keeps_its_folders_and_index_script() {
        let root = site(&[
            ("scripts/generate-blog-data.js", ""),
            ("publishkit.yaml", "site_name: Old\nsite_url: https://old.org\nauthor: Me\ndefault_tags: []\n\
                system_prompt: Mine\nsocial_enabled: false\ncontent_paths:\n  langs:\n    fr:\n      queue: drafts/fr\n      blog: out/fr\n"),
        ]);
        let project = root.to_string_lossy();
        let cfg = ProjectConfig::load(&project);
        assert_eq!(cfg.preset, Some(Preset::PublishKit));
        assert_eq!(cfg.system_prompt, "Mine");
        assert_eq!(cfg.content_paths.unwrap().langs["fr"].queue, "drafts/fr");
        assert_eq!(cfg.publish_command, "node scripts/generate-blog-data.js");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_config_that_does_not_parse_is_never_written_over() {
        let broken = "site_name: [unclosed\n";
        let root = site(&[("publishkit.yaml", broken), ("posts/a.md", "---\ntitle: A\n---\n")]);
        let project = root.to_string_lossy();
        let _ = ProjectConfig::load(&project);
        assert_eq!(std::fs::read_to_string(root.join("publishkit.yaml")).unwrap(), broken);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn page_bundles_are_posts_and_list_pages_are_not() {
        let root = site(&[
            ("astro.config.mjs", ""),
            ("src/content/blog/_index.md", "---\ntitle: List\n---\n"),
            ("src/content/blog/one/index.md", "---\ntitle: One\ndate: 2026-01-02\n---\n"),
            ("src/content/blog/one/cover.png", ""),
            ("src/content/blog/two/index.mdx", "---\ntitle: Two\ndate: 2026-01-01\ndraft: true\n---\n"),
            ("src/content/blog/notes/readme.txt", ""),
        ]);
        let project = root.to_string_lossy();
        let cfg = ProjectConfig::load(&project);
        assert!(cfg.bundles);
        let shelf = load_posts(&project).blog("en");
        assert_eq!(shelf.published.iter().map(|p| p.filename.as_str()).collect::<Vec<_>>(), ["one/index.md"]);
        assert_eq!(shelf.queue.iter().map(|p| p.filename.as_str()).collect::<Vec<_>>(), ["two/index.mdx"]);
        assert_eq!(slug_of("one/index.md"), "one");
        let stamp = content_stamp(&project);
        assert!(stamp.iter().any(|(p, _)| p.ends_with("one/index.md")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_open_site_comes_first_and_gone_folders_are_left_out() {
        let a = site(&[("publishkit.yaml", "site_name: Alpha\n")]);
        let b = site(&[("x", "")]);
        let (a_path, b_path) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned());
        let mut cfg = AppConfig { project_path: Some(a_path.clone()), ..Default::default() };
        assert_eq!(cfg.sites(), [a_path.clone()], "a site open before the list existed is in it");
        cfg.open_project(&b_path);
        cfg.open_project("/no/such/folder");
        cfg.open_project(&b_path);
        assert_eq!(cfg.sites(), [b_path.clone(), a_path.clone()]);
        assert_eq!(site_label(&a_path), "Alpha");
        assert_eq!(site_label(&b_path), b.file_name().unwrap().to_string_lossy());
        cfg.forget_project(&b_path);
        assert_eq!(cfg.project_path.as_deref(), Some(a_path.as_str()));
        std::fs::remove_dir_all(a).unwrap();
        std::fs::remove_dir_all(b).unwrap();
    }

    #[test]
    fn languages_marked_in_file_names_are_languages() {
        let root = site(&[
            ("hugo.toml", "baseURL = 'https://x.org'\ndefaultContentLanguage = 'fr'\n"),
            ("content/posts/a.md", "+++\ntitle = \"A fr\"\n+++\n"),
            ("content/posts/a.en.md", "+++\ntitle = \"A en\"\n+++\n"),
            ("content/posts/b.en.md", "+++\ntitle = \"B en\"\ndraft = true\n+++\n"),
            ("content/posts/c/index.md", "+++\ntitle = \"C fr\"\n+++\n"),
            ("content/posts/c/index.en.md", "+++\ntitle = \"C en\"\n+++\n"),
            ("content/posts/v1.2.md", "+++\ntitle = \"Version\"\n+++\n"),
        ]);
        let project = root.to_string_lossy();
        let cfg = ProjectConfig::load(&project);
        assert_eq!(cfg.languages(), ["en", "fr"]);
        assert_eq!((cfg.language_mark("en"), cfg.language_mark("fr")), ("en", ""));
        let posts = load_posts(&project);
        let titles = |lang: &str| {
            let shelf = posts.blog(lang);
            let mut t: Vec<String> = shelf.queue.iter().chain(&shelf.published).map(|p| p.title.clone()).collect();
            t.sort();
            t
        };
        assert_eq!(titles("fr"), ["A fr", "C fr", "Version"]);
        assert_eq!(titles("en"), ["A en", "B en", "C en"]);
        assert_eq!(cfg.counterpart("a.md", "fr", "en"), "a.en.md");
        assert_eq!(cfg.counterpart("b.en.md", "en", "fr"), "b.md");
        assert_eq!(cfg.counterpart("c/index.md", "fr", "en"), "c/index.en.md");
        assert_eq!(slug_of("a.en.md"), "a");
        assert_eq!(slug_of("v1.2.md"), "v1.2");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn slugs_and_addresses() {
        assert_eq!(slug_of("2026-01-02-hello-world.md"), "hello-world");
        assert_eq!(slug_of("hello.mdx"), "hello");
        let cfg = ProjectConfig { site_url: "https://x.org/".into(), post_url: "{site_url}/{lang}/p/{slug}/".into(), ..Default::default() };
        assert_eq!(cfg.post_url("de", "a-b.md"), "https://x.org/de/p/a-b/");
        assert!(is_lang_code("en") && is_lang_code("pt-br") && !is_lang_code("blog") && !is_lang_code("img1"));
    }
}

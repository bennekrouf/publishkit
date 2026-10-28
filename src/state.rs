use crate::frontmatter::{self, Format};
use crate::plan::{Plan, Stage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
mod posts;
mod project;
pub use posts::*;
mod detect;
pub use detect::*;

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
        Self {
            queue: vec![],
            published: vec![],
        }
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
        Self {
            languages: vec![],
            blog: BTreeMap::new(),
            linkedin: BTreeMap::new(),
        }
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
        "en" => "English",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "da" => "Danish",
        "no" | "nb" => "Norwegian",
        "fi" => "Finnish",
        "pl" => "Polish",
        "cs" => "Czech",
        "ro" => "Romanian",
        "hu" => "Hungarian",
        "el" => "Greek",
        "tr" => "Turkish",
        "ru" => "Russian",
        "uk" => "Ukrainian",
        "ar" => "Arabic",
        "he" => "Hebrew",
        "fa" => "Persian",
        "hi" => "Hindi",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        "id" => "Indonesian",
        "vi" => "Vietnamese",
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
        self.region().map_or("api0", |r| r.shown())
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
        LlmProvider::Api0 {
            region: region.to_string(),
        }
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
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

impl AppConfig {
    pub fn config_path() -> PathBuf {
        // Still the old name: the API keys are saved here, and an earlier rename
        // lost them by moving it.
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
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
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
        Preset::PublishKit,
        Preset::Hugo,
        Preset::Jekyll,
        Preset::Astro,
        Preset::Eleventy,
        Preset::Zola,
        Preset::Docusaurus,
        Preset::Next,
        Preset::Generic,
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
        .and_then(|v| {
            v.get("site_name")
                .and_then(|n| n.as_str())
                .map(|n| n.trim().to_string())
        })
        .filter(|n| !n.is_empty());
    named.unwrap_or_else(|| {
        Path::new(project_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| project_path.to_string())
    })
}

/// The project's config file: `publishkit.yaml`, or `blog-toolkit.yaml` in a
/// project set up before the app was renamed PublishKit. That file is kept
/// rather than renamed: it sits in the person's own repository.
/// A folder from publishkit.yaml, always written with `/`, as a path in this platform's
/// separators: joined as-is on Windows it gives `C:\\site\\content/en/queue`.
fn native(relative: &str) -> PathBuf {
    relative
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect()
}

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

/// `2026-01-02-`
pub fn is_date_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 11
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 | 10 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

fn is_post_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|x| x.to_str()),
        Some("md" | "mdx")
    )
}

/// The file of a page bundle, a folder holding a post beside its images: `index.md`, or
/// `index.fr.md` and so on where the file name marks the language.
pub fn is_bundle_index(name: &str) -> bool {
    name.starts_with("index.") && (name.ends_with(".md") || name.ends_with(".mdx"))
}

/// A post's name in its folder: `my-post.md`, or `my-post/index.md` for a bundle.
fn post_name(dir: &Path, path: &Path) -> Option<String> {
    let name = path
        .strip_prefix(dir)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    Some(name)
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

/// `fr` for `post.fr.md`, when what sits before the extension looks like a language.
fn marked_language(name: &str) -> Option<&str> {
    let stem = name
        .strip_suffix(".md")
        .or_else(|| name.strip_suffix(".mdx"))?;
    let (_, mark) = stem.rsplit_once('.')?;
    is_lang_code(mark).then_some(mark)
}

/// A one-language site's language, from the `lang` of its first post.
fn site_language(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(first_post(dir)?).ok()?;
    let fields = frontmatter::fields(&content);
    let lang =
        frontmatter::text(&fields, "lang").or_else(|| frontmatter::text(&fields, "language"))?;
    is_lang_code(&lang.to_lowercase()).then(|| lang.to_lowercase())
}

/// `en`, `fra`, `pt-br`: a folder named after a language, not `blog` or `docs`.
fn is_lang_code(name: &str) -> bool {
    let (base, region) = name
        .split_once('-')
        .map_or((name, None), |(b, r)| (b, Some(r)));
    (2..=3).contains(&base.len())
        && base.chars().all(|c| c.is_ascii_lowercase())
        && region.is_none_or(|r| {
            (2..=4).contains(&r.len()) && r.chars().all(|c| c.is_ascii_alphanumeric())
        })
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

#[cfg(test)]
mod tests;

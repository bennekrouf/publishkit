use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SocialPosts {
    pub linkedin: String,
    pub twitter: String,
}

/// A standalone LinkedIn post about a product/feature (not tied to a blog article).
#[derive(Clone, Debug, PartialEq)]
pub struct LinkedInPost {
    pub filename: String,
    pub topic: String,
    pub lang: String,
    pub status: String,
    pub date: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub filename: String,
    pub title: String,
    pub description: String,
    pub date: String,
    pub lang: String,
    pub status: String,
    pub body: String,
    pub full_content: String,
    pub social: Option<SocialPosts>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectPosts {
    pub queue_fr: Vec<Post>,
    pub queue_en: Vec<Post>,
    pub published_fr: Vec<Post>,
    pub published_en: Vec<Post>,
    pub linkedin_queue_fr: Vec<LinkedInPost>,
    pub linkedin_queue_en: Vec<LinkedInPost>,
    pub linkedin_published_fr: Vec<LinkedInPost>,
    pub linkedin_published_en: Vec<LinkedInPost>,
}

impl ProjectPosts {
    pub fn empty() -> Self {
        Self {
            queue_fr: vec![], queue_en: vec![], published_fr: vec![], published_en: vec![],
            linkedin_queue_fr: vec![], linkedin_queue_en: vec![],
            linkedin_published_fr: vec![], linkedin_published_en: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum LlmProvider {
    #[default]
    DeepSeek,
    Claude,
}

impl LlmProvider {
    pub fn label(&self) -> &'static str {
        match self { LlmProvider::DeepSeek => "DeepSeek", LlmProvider::Claude => "Claude" }
    }
    pub fn model(&self) -> &'static str {
        match self {
            LlmProvider::DeepSeek => "deepseek-chat",
            LlmProvider::Claude   => "claude-sonnet-4-6",
        }
    }
    pub fn api_url(&self) -> &'static str {
        match self {
            LlmProvider::DeepSeek => "https://api.deepseek.com/chat/completions",
            LlmProvider::Claude   => "https://api.anthropic.com/v1/messages",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub project_path: Option<String>,
    pub provider: LlmProvider,
    pub api_key: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self { project_path: None, provider: LlmProvider::default(), api_key: None }
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

    pub fn resolved_api_key(&self) -> String {
        if let Some(k) = &self.api_key {
            if !k.is_empty() { return k.clone(); }
        }
        // fallback to env
        match self.provider {
            LlmProvider::DeepSeek => std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
            LlmProvider::Claude   => std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
        }
    }
}

/// Detected content directory layout per language.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LangPaths {
    pub queue: String,
    pub blog: String,
}

/// Mapping of language codes to their content paths (relative to project root).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ContentPaths {
    pub langs: HashMap<String, LangPaths>,
}

/// Per-project config loaded from `publishkit.yaml` at the project root.
/// If absent, defaults to Cvenom-style settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub site_name: String,
    pub site_url: String,
    pub author: String,
    pub default_tags: Vec<String>,
    pub system_prompt: String,
    pub social_enabled: bool,
    #[serde(default)]
    pub content_paths: Option<ContentPaths>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            site_name: "Cvenom".to_string(),
            site_url: "https://cvenom.com".to_string(),
            author: "Cvenom Team".to_string(),
            default_tags: vec!["carrière".to_string(), "développement-professionnel".to_string()],
            social_enabled: true,
            system_prompt: "You are a professional blog writer for Cvenom, a career and CV platform. \
                You write engaging, insightful posts about career development, workplace psychology, \
                and professional growth. \
                Your tone is empathetic, intelligent, and slightly provocative — like a good career coach. \
                Avoid corporate jargon. Use concrete examples and reference established frameworks \
                (Holland's theory, Person-Environment Fit, etc.) when relevant.".to_string(),
            content_paths: None,
        }
    }
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

impl ProjectConfig {
    pub fn load(project_path: &str) -> Self {
        let path = config_path(project_path);
        let mut config: Self = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_default();

        // Auto-detect content paths if not already configured
        if config.content_paths.is_none() {
            let detected = detect_structure(project_path);
            if !detected.langs.is_empty() {
                config.content_paths = Some(detected);
                // Persist detection to yaml so it only runs once
                config.save(project_path);
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

    /// Paths for standalone LinkedIn posts: content/{lang}/linkedin/{queue,published}
    pub fn resolve_linkedin_paths(&self, project_path: &str, lang: &str) -> (PathBuf, PathBuf) {
        let root = PathBuf::from(project_path).join("content").join(lang).join("linkedin");
        (root.join("queue"), root.join("published"))
    }

    /// Get all detected languages, or default to ["fr", "en"].
    pub fn languages(&self) -> Vec<String> {
        if let Some(cp) = &self.content_paths {
            if !cp.langs.is_empty() {
                return cp.langs.keys().cloned().collect();
            }
        }
        vec!["fr".to_string(), "en".to_string()]
    }
}

/// Every post file of the project with its modification time, sorted: when it differs from the
/// last one, a post was added, changed, moved or deleted, here or by `publishkit mcp`.
pub fn content_stamp(project_path: &str) -> Vec<(PathBuf, std::time::SystemTime)> {
    let config = ProjectConfig::load(project_path);
    let mut stamp = Vec::new();
    for lang in config.languages() {
        let (queue, blog) = config.resolve_paths(project_path, &lang);
        let (li_queue, li_published) = config.resolve_linkedin_paths(project_path, &lang);
        for dir in [queue, blog, li_queue, li_published] {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.filter_map(|e| e.ok()) {
                if let Ok(modified) = e.metadata().and_then(|m| m.modified()) {
                    stamp.push((e.path(), modified));
                }
            }
        }
    }
    stamp.sort();
    stamp
}

pub fn load_posts(project_path: &str) -> ProjectPosts {
    let config = ProjectConfig::load(project_path);
    let (queue_fr, blog_fr) = config.resolve_paths(project_path, "fr");
    let (queue_en, blog_en) = config.resolve_paths(project_path, "en");
    let (li_queue_fr, li_pub_fr) = config.resolve_linkedin_paths(project_path, "fr");
    let (li_queue_en, li_pub_en) = config.resolve_linkedin_paths(project_path, "en");
    ProjectPosts {
        queue_fr: read_posts(&queue_fr),
        queue_en: read_posts(&queue_en),
        published_fr: read_posts(&blog_fr),
        published_en: read_posts(&blog_en),
        linkedin_queue_fr: read_linkedin_posts(&li_queue_fr),
        linkedin_queue_en: read_linkedin_posts(&li_queue_en),
        linkedin_published_fr: read_linkedin_posts(&li_pub_fr),
        linkedin_published_en: read_linkedin_posts(&li_pub_en),
    }
}

fn read_linkedin_posts(dir: &PathBuf) -> Vec<LinkedInPost> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut posts: Vec<LinkedInPost> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
        .filter_map(|e| parse_linkedin_post(e.path()))
        .collect();
    posts.sort_by(|a, b| b.date.cmp(&a.date));
    posts
}

fn parse_linkedin_post(path: PathBuf) -> Option<LinkedInPost> {
    let full_content = std::fs::read_to_string(&path).ok()?;
    let filename = path.file_name()?.to_string_lossy().to_string();

    let (frontmatter, body) = if full_content.starts_with("---") {
        let rest = &full_content[3..];
        if let Some(end) = rest.find("\n---") {
            (rest[..end].to_string(), rest[end + 4..].trim_start_matches('\n').to_string())
        } else {
            (String::new(), full_content.clone())
        }
    } else {
        (String::new(), full_content.clone())
    };

    let topic  = extract_field(&frontmatter, "topic").unwrap_or_else(|| filename.clone());
    let date   = extract_field(&frontmatter, "date").unwrap_or_default();
    let lang   = extract_field(&frontmatter, "lang").unwrap_or_else(|| "fr".to_string());
    let status = extract_field(&frontmatter, "status").unwrap_or_else(|| "draft".to_string());

    Some(LinkedInPost { filename, topic, lang, status, date, body })
}

/// Scan project directory to detect content folder structure.
/// Looks for directories containing .md files with YAML frontmatter.
/// Recognizes patterns like:
///   content/{lang}/queue/, content/{lang}/blog/
///   content/{lang}/draft/, content/{lang}/published/
///   posts/, _posts/, blog/
fn detect_structure(project_path: &str) -> ContentPaths {
    let root = PathBuf::from(project_path);
    let mut langs: HashMap<String, LangPaths> = HashMap::new();

    // Strategy 1: content/{lang}/{queue|draft|blog|published}/
    let content_dir = root.join("content");
    if content_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&content_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let lang = path.file_name().unwrap().to_string_lossy().to_string();
                // Skip non-language-looking dirs (must be 2-3 chars)
                if lang.len() < 2 || lang.len() > 3 { continue; }

                let queue_dir = find_queue_dir(&path);
                let blog_dir = find_blog_dir(&path);

                if queue_dir.is_some() || blog_dir.is_some() {
                    let base = format!("content/{lang}");
                    langs.insert(lang, LangPaths {
                        queue: queue_dir.unwrap_or_else(|| format!("{base}/queue")),
                        blog: blog_dir.unwrap_or_else(|| format!("{base}/blog")),
                    });
                }
            }
        }
    }

    // Strategy 2: if no content/{lang} found, look for top-level blog dirs
    if langs.is_empty() {
        let candidates = ["posts", "_posts", "blog", "articles"];
        for name in candidates {
            let dir = root.join(name);
            if dir.is_dir() && dir_has_md_with_frontmatter(&dir) {
                // Single-language project, default to "fr"
                langs.insert("fr".to_string(), LangPaths {
                    queue: "queue".to_string(),
                    blog: name.to_string(),
                });
                break;
            }
        }
    }

    ContentPaths { langs }
}

fn find_queue_dir(lang_dir: &PathBuf) -> Option<String> {
    let lang = lang_dir.file_name()?.to_string_lossy().to_string();
    for name in ["queue", "draft", "drafts"] {
        let dir = lang_dir.join(name);
        if dir.is_dir() {
            return Some(format!("content/{lang}/{name}"));
        }
    }
    None
}

fn find_blog_dir(lang_dir: &PathBuf) -> Option<String> {
    let lang = lang_dir.file_name()?.to_string_lossy().to_string();
    for name in ["blog", "published", "posts"] {
        let dir = lang_dir.join(name);
        if dir.is_dir() && dir_has_md_with_frontmatter(&dir) {
            return Some(format!("content/{lang}/{name}"));
        }
    }
    // If no published dir exists yet but queue does, default to "blog"
    None
}

fn dir_has_md_with_frontmatter(dir: &PathBuf) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
        .take(1)
        .any(|e| {
            std::fs::read_to_string(e.path())
                .map(|c| c.starts_with("---"))
                .unwrap_or(false)
        })
}

fn read_posts(dir: &PathBuf) -> Vec<Post> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut posts: Vec<Post> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
        .filter_map(|e| parse_post(e.path()))
        .collect();
    posts.sort_by(|a, b| a.filename.cmp(&b.filename));
    posts
}

fn parse_post(path: PathBuf) -> Option<Post> {
    let full_content = std::fs::read_to_string(&path).ok()?;
    let filename = path.file_name()?.to_string_lossy().to_string();

    let (frontmatter, body) = if full_content.starts_with("---") {
        let rest = &full_content[3..];
        if let Some(end) = rest.find("\n---") {
            (rest[..end].to_string(), rest[end + 4..].trim_start_matches('\n').to_string())
        } else {
            (String::new(), full_content.clone())
        }
    } else {
        (String::new(), full_content.clone())
    };

    let title       = extract_field(&frontmatter, "title").unwrap_or_else(|| filename.clone());
    let description = extract_field(&frontmatter, "description").unwrap_or_default();
    let date        = extract_field(&frontmatter, "date").unwrap_or_default();
    let lang        = extract_field(&frontmatter, "lang").unwrap_or_else(|| "fr".to_string());
    let status      = extract_field(&frontmatter, "status").unwrap_or_else(|| "draft".to_string());

    // Load sidecar social file if it exists
    let social_path = path.with_extension("social.json");
    let social = std::fs::read_to_string(&social_path)
        .ok()
        .and_then(|s| serde_json::from_str::<SocialPosts>(&s).ok());

    Some(Post { filename, title, description, date, lang, status, body, full_content, social })
}

fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
            return Some(rest.trim().trim_matches('"').to_string());
        }
    }
    None
}

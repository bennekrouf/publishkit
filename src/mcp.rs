//! `publishkit mcp` — the blog's posts, served to an MCP client over stdio, and the window that
//! says how to connect one.
//!
//! No window: the same files and the same functions the app uses (`state::load_posts`,
//! `api::save_to_queue`, `api::publish_post`, …), driven from tool calls instead. Posts are the
//! project's markdown files, which the app may have open too; it reloads its lists when they
//! change (see the watcher in `MainLayout`).
//!
//! Claude usually writes a post itself; the app's own model (DeepSeek or Claude, set in its
//! Settings) is only called when a tool is asked to generate.

use crate::api;
use crate::frontmatter;
use crate::plan::{self, Stage};
use crate::state::{AppConfig, LinkedInPost, Post, ProjectConfig, SocialPosts, load_posts};
use dioxus::prelude::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// The newest protocol revision this server speaks. A client asking for an older one gets its
/// own back; everything used here exists in all of them.
const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "PublishKit manages a website's blog as markdown files: drafts wait, per language, \
until they are published, and LinkedIn posts the same way. Start with project, which gives the site, the \
framework it is built with, its languages and how many posts each has; tools default to the project open in \
PublishKit. list_posts and get_post read posts. queue_post adds a draft: write its body yourself in the site's \
voice (project gives the site's own writing brief), or pass generate: true to have PublishKit's configured model \
write it. update_post edits a draft; set_social sets the LinkedIn and X teasers of a post. publish_post makes a \
draft live: it is dated today and the site's publish command, if it has one, runs, so ask the user before \
publishing, and before delete_post. schedule_post sets a draft's stage (idea, draft, review) and the date and \
time it is published by itself; ask the user before scheduling. queue_linkedin adds a standalone LinkedIn post.";

pub fn serve() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        let mut stdout = tokio::io::stdout();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                write(&mut stdout, &error(Value::Null, -32700, "not JSON")).await;
                continue;
            };
            // A notification (no id) asks for nothing back.
            let Some(id) = message.get("id").cloned() else { continue };
            let reply = handle(id, &message).await;
            write(&mut stdout, &reply).await;
        }
    });
}

async fn handle(id: Value, message: &Value) -> Value {
    let method = message["method"].as_str().unwrap_or("");
    let params = &message["params"];
    match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or(PROTOCOL);
            let version = if asked <= PROTOCOL { asked } else { PROTOCOL };
            result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "publishkit", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let (text, is_error) = match call(name, &params["arguments"]).await {
                Ok(value) => (pretty(&value), false),
                Err(why) => (why, true),
            };
            result(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
        }
        _ => error(id, -32601, &format!("no method {method}")),
    }
}

async fn write(stdout: &mut tokio::io::Stdout, reply: &Value) {
    // One message per line, and nothing else ever on stdout: the client reads it as the
    // protocol, so a stray print is a corrupt message.
    let mut bytes = serde_json::to_vec(reply).unwrap_or_default();
    bytes.push(b'\n');
    let _ = stdout.write_all(&bytes).await;
    let _ = stdout.flush().await;
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

// ── Tool definitions ─────────────────────────────────────────────────────────────────────

fn tools() -> Value {
    let project = json!({ "type": "string", "description": "Absolute path of the site's folder. Defaults to the one open in PublishKit." });
    let lang = json!({ "type": "string", "description": "Language code, e.g. fr or en (project lists them)." });
    let filename = json!({ "type": "string", "description": "The post's file name as list_posts gives it: my-post.md, my-post.mdx, or my-post/index.md for a page bundle." });
    let kind = json!({ "type": "string", "enum": ["blog", "linkedin"], "description": "Default blog." });
    let place = json!({ "type": "string", "enum": ["queue", "published"], "description": "Default queue." });
    json!([
        {
            "name": "project",
            "description": "The site: its name, URL, author, default tags, its writing brief (the system prompt \
                            PublishKit writes with), the framework it is built with, its languages, and how many \
                            drafts and published posts each has, blog and LinkedIn. Also the model PublishKit \
                            generates with, and the other sites PublishKit knows (pass one's folder as project).",
            "inputSchema": { "type": "object", "properties": { "project": project } },
        },
        {
            "name": "list_posts",
            "description": "Posts with their title (or LinkedIn topic), description, date, status and length.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": { "type": "string", "description": "One language; default all." },
                    "kind": kind,
                    "where": { "type": "string", "enum": ["queue", "published", "all"], "description": "Default all." },
                },
            },
        },
        {
            "name": "get_post",
            "description": "One post in full: its front matter, its markdown, and its social teasers.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "lang": lang, "filename": filename, "kind": kind, "where": place },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "queue_post",
            "description": "Adds a blog draft to a language's queue, with the site's author and default tags. When \
                            project says title_heading, the file starts with the title as a heading, so don't repeat \
                            it in the body. Give body, or \
                            generate: true to have PublishKit's model write it from the title and summary. social \
                            adds the teasers; generate_social has PublishKit's model write them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": lang,
                    "title": { "type": "string" },
                    "summary": { "type": "string", "description": "One or two sentences: the post's description." },
                    "body": { "type": "string", "description": "Markdown: intro, ## sections, conclusion." },
                    "generate": { "type": "boolean" },
                    "social": {
                        "type": "object",
                        "properties": { "linkedin": { "type": "string" }, "twitter": { "type": "string" } },
                    },
                    "generate_social": { "type": "boolean" },
                },
                "required": ["lang", "title", "summary"],
            },
        },
        {
            "name": "update_post",
            "description": "Changes a draft (blog or LinkedIn): body replaces what follows the front matter, \
                            content replaces the whole file (front matter included). Published posts aren't edited here.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind,
                    "body": { "type": "string" },
                    "content": { "type": "string" },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "set_social",
            "description": "Sets a blog post's LinkedIn and X teasers, or has PublishKit's model write them \
                            (generate: true), with the post's URL on the site.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "where": place,
                    "linkedin": { "type": "string" },
                    "twitter": { "type": "string", "description": "At most 280 characters, the URL included." },
                    "generate": { "type": "boolean" },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "publish_post",
            "description": "Publishes a draft: moves it into the blog (or LinkedIn's published folder), or clears its \
                            draft mark where drafts sit with published posts, dates it today, and for a blog post \
                            runs the site's publish command, if it has one. A blog post whose checks find problems \
                            (no title or text, TODO left in, a missing image) is refused with them; get_post lists \
                            its checks. Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind,
                    "force": { "type": "boolean", "description": "Publish a blog post although its checks found problems. Only when the user says so." },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "schedule_post",
            "description": "Sets a blog draft's stage on PublishKit's board (idea, draft or review) and when it is \
                            published by itself: publish_at, local time like 2026-11-02T09:00, or an empty string \
                            to unschedule. PublishKit publishes it then while its window is open, or when \
                            `publishkit publish-due` runs. Ask the user before scheduling.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename,
                    "stage": { "type": "string", "enum": ["idea", "draft", "review"] },
                    "publish_at": { "type": "string" },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "crosspost_post",
            "description": "Sends a published blog post to dev.to, Hashnode, a WordPress or a Ghost blog set up in \
                            PublishKit's Settings, with the site's address as canonical — or updates the copy sent \
                            before. Copies go as drafts there unless the user turned that off. Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename,
                    "target": { "type": "string", "enum": ["devto", "hashnode", "wordpress", "ghost"] },
                },
                "required": ["lang", "filename", "target"],
            },
        },
        {
            "name": "delete_post",
            "description": "Deletes a queued draft and its teasers. Published posts aren't deleted here. Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "lang": lang, "filename": filename, "kind": kind },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "queue_linkedin",
            "description": "Adds a standalone LinkedIn post to a language's queue. Give body, or generate: true with an \
                            angle to have PublishKit's model write it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": lang,
                    "topic": { "type": "string" },
                    "body": { "type": "string" },
                    "angle": { "type": "string", "description": "Key points, for generate." },
                    "generate": { "type": "boolean" },
                },
                "required": ["lang", "topic"],
            },
        },
    ])
}

async fn call(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "project" => project_info(args),
        "list_posts" => list_posts(args),
        "get_post" => get_post(args),
        "queue_post" => queue_post(args).await,
        "update_post" => update_post(args),
        "set_social" => set_social(args).await,
        "publish_post" => publish(args),
        "delete_post" => delete(args),
        "schedule_post" => schedule(args),
        "crosspost_post" => crosspost(args).await,
        "queue_linkedin" => queue_linkedin(args).await,
        _ => Err(format!("no tool {name}")),
    }
}

// ── Arguments ────────────────────────────────────────────────────────────────────────────

fn text(args: &Value, key: &str) -> Result<String, String> {
    args[key].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from).ok_or(format!("{key} is required"))
}

/// The site's folder: given, or the one open in PublishKit.
fn project_path(args: &Value) -> Result<String, String> {
    let path = match args["project"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => p.to_string(),
        None => AppConfig::load()
            .project_path
            .filter(|p| !p.is_empty())
            .ok_or("no site is open in PublishKit: pass project, the site's folder")?,
    };
    if !Path::new(&path).is_absolute() || !Path::new(&path).is_dir() {
        return Err(format!("{path} is not a folder (an absolute path)"));
    }
    Ok(path)
}

fn language(args: &Value, cfg: &ProjectConfig) -> Result<String, String> {
    let lang = text(args, "lang")?.to_lowercase();
    let langs = cfg.languages();
    if !langs.contains(&lang) {
        return Err(format!("no language {lang} in this site; it has {}", langs.join(", ")));
    }
    Ok(lang)
}

/// A post's name in its posts folder: a plain `.md` or `.mdx` name, or a page bundle's
/// `folder/index.md` — never a path out of the folder.
fn filename(args: &Value) -> Result<String, String> {
    let name = text(args, "filename")?;
    let plain = |n: &str| !n.contains(['/', '\\']) && !n.starts_with('.') && (n.ends_with(".md") || n.ends_with(".mdx"));
    let ok = match name.split_once('/') {
        Some((folder, file)) => {
            !folder.is_empty() && !folder.starts_with('.') && !folder.contains(['/', '\\']) && crate::state::is_bundle_index(file) && !file.contains('/')
        }
        None => plain(&name),
    };
    if !ok {
        return Err(format!("{name} is not a post's file name (like my-post.md, or my-post/index.md for a bundle)"));
    }
    Ok(name)
}

fn linkedin(args: &Value) -> Result<bool, String> {
    match args["kind"].as_str().unwrap_or("blog") {
        "blog" => Ok(false),
        "linkedin" => Ok(true),
        other => Err(format!("kind is blog or linkedin, not {other}")),
    }
}

fn published(args: &Value) -> Result<bool, String> {
    match args["where"].as_str().unwrap_or("queue") {
        "queue" => Ok(false),
        "published" => Ok(true),
        other => Err(format!("where is queue or published, not {other}")),
    }
}

/// The folder a post is in.
fn folder(project: &str, cfg: &ProjectConfig, lang: &str, linkedin: bool, published: bool) -> PathBuf {
    let (queue, done) =
        if linkedin { cfg.resolve_linkedin_paths(project, lang) } else { cfg.resolve_paths(project, lang) };
    if published { done } else { queue }
}

/// The model PublishKit generates with, and its key; refused when there is no key.
fn model() -> Result<(AppConfig, String), String> {
    let cfg = AppConfig::load();
    let key = cfg.resolved_api_key();
    if key.is_empty() {
        return Err(format!(
            "{}. Or write the text yourself and pass it instead",
            cfg.missing_key().trim_end_matches('.')
        ));
    }
    Ok((cfg, key))
}

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn post_row(p: &Post) -> Value {
    json!({
        "filename": p.filename,
        "title": p.title,
        "description": p.description,
        "date": p.date,
        "status": p.status,
        "tags": p.tags,
        "stage": p.is_draft().then(|| p.stage.key()),
        "publish_at": p.publish_at,
        "words": words(&p.body),
        "has_social": p.social.is_some(),
    })
}

fn linkedin_row(p: &LinkedInPost) -> Value {
    json!({ "filename": p.filename, "topic": p.topic, "date": p.date, "status": p.status, "words": words(&p.body) })
}

// ── Tools ────────────────────────────────────────────────────────────────────────────────

fn project_info(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let posts = load_posts(&project);
    let mut counts = serde_json::Map::new();
    for lang in &posts.languages {
        let (blog, linkedin) = (posts.blog(lang), posts.linkedin(lang));
        counts.insert(
            lang.clone(),
            json!({
                "drafts": blog.queue.len(),
                "published": blog.published.len(),
                "linkedin_drafts": linkedin.queue.len(),
                "linkedin_published": linkedin.published.len(),
            }),
        );
    }
    let app = AppConfig::load();
    let other_sites: Vec<Value> = app
        .sites()
        .into_iter()
        .filter(|p| *p != project)
        .map(|p| json!({ "project": p, "site_name": crate::state::site_label(&p) }))
        .collect();
    Ok(json!({
        "project": project,
        "other_sites": other_sites,
        "site_name": cfg.site_name,
        "site_url": cfg.site_url,
        "author": cfg.author,
        "default_tags": cfg.default_tags,
        "writing_brief": cfg.system_prompt,
        "social_teasers": cfg.social_enabled,
        "framework": cfg.preset.map(|p| p.label()),
        "title_heading": cfg.title_heading,
        "publish_command": cfg.publish_command,
        "languages": counts,
        "model": match app.provider.region() {
            Some(r) => r.display.to_string(),
            None => format!("{} ({})", app.provider.label(), app.provider.model()),
        },
        "model_key_set": !app.resolved_api_key().is_empty(),
    }))
}

/// Posts of one kind and place in one language.
fn posts_in(project: &str, lang: &str, linkedin: bool, published: bool) -> Vec<Value> {
    let all = load_posts(project);
    if linkedin {
        let shelf = all.linkedin(lang);
        let posts = if published { &shelf.published } else { &shelf.queue };
        posts.iter().map(linkedin_row).collect()
    } else {
        let shelf = all.blog(lang);
        let posts = if published { &shelf.published } else { &shelf.queue };
        posts.iter().map(post_row).collect()
    }
}

/// A blog draft's path, refused when there is no such draft: where drafts sit with published
/// posts, the file being there isn't enough.
fn draft_path(project: &str, cfg: &ProjectConfig, lang: &str, name: &str, linkedin: bool) -> Result<PathBuf, String> {
    let path = folder(project, cfg, lang, linkedin, false).join(name);
    let is_draft = |p: &Path| {
        linkedin
            || cfg.drafts == crate::state::Drafts::Folder
            || std::fs::read_to_string(p).is_ok_and(|c| frontmatter::flag(&frontmatter::fields(&c), "draft") == Some(true))
    };
    if path.is_file() && is_draft(&path) {
        Ok(path)
    } else {
        Err(format!("no draft {name} in {lang}"))
    }
}

fn list_posts(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let linkedin = linkedin(args)?;
    let langs = match args["lang"].as_str() {
        Some(_) => vec![language(args, &cfg)?],
        None => cfg.languages(),
    };
    let (queue, done) = match args["where"].as_str().unwrap_or("all") {
        "all" => (true, true),
        "queue" => (true, false),
        "published" => (false, true),
        other => return Err(format!("where is queue, published or all, not {other}")),
    };
    let mut out = serde_json::Map::new();
    for lang in langs {
        let mut v = serde_json::Map::new();
        if queue {
            v.insert("queue".into(), Value::Array(posts_in(&project, &lang, linkedin, false)));
        }
        if done {
            v.insert("published".into(), Value::Array(posts_in(&project, &lang, linkedin, true)));
        }
        out.insert(lang, Value::Object(v));
    }
    Ok(json!({ "kind": if linkedin { "linkedin" } else { "blog" }, "posts": out }))
}

fn get_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let path = folder(&project, &cfg, &lang, linkedin(args)?, published(args)?).join(&name);
    let content = std::fs::read_to_string(&path).map_err(|_| format!("no post {name} there ({})", path.display()))?;
    let social: Option<SocialPosts> = std::fs::read_to_string(path.with_extension("social.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let front_matter = frontmatter::fields(&content);
    let checks: Vec<Value> = load_posts(&project)
        .blog(&lang)
        .queue
        .into_iter()
        .chain(load_posts(&project).blog(&lang).published)
        .find(|p| p.path == path)
        .map(|post| crate::checks::check(&post, &project, &cfg))
        .unwrap_or_default()
        .into_iter()
        .map(|i| json!({ "level": if i.level == crate::checks::Level::Problem { "problem" } else { "warning" }, "message": i.message }))
        .collect();
    Ok(json!({ "path": path, "front_matter": front_matter, "content": content, "words": words(&content), "social": social, "checks": checks }))
}

async fn queue_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let title = text(args, "title")?;
    let summary = text(args, "summary")?;
    let body = match args["body"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(b) => b.to_string(),
        None if args["generate"].as_bool().unwrap_or(false) => {
            let (app, key) = model()?;
            api::generate_post(&title, &summary, &lang, &app.provider, &key, &cfg).await.map_err(|e| format!("{e:#}"))?
        }
        None => return Err("give body, or generate: true to have PublishKit's model write it".into()),
    };
    let path = api::save_to_queue(&project, &title, &summary, &lang, &body, &cfg).map_err(|e| format!("{e:#}"))?;
    let social = if let Some(s) = args.get("social").filter(|s| s.is_object()) {
        Some(SocialPosts {
            linkedin: s["linkedin"].as_str().unwrap_or("").to_string(),
            twitter: s["twitter"].as_str().unwrap_or("").to_string(),
        })
    } else if args["generate_social"].as_bool().unwrap_or(false) {
        let (app, key) = model()?;
        let url = post_url(&cfg, &lang, &path);
        Some(api::generate_social_posts(&title, &summary, &lang, &url, &app.provider, &key).await.map_err(|e| format!("{e:#}"))?)
    } else {
        None
    };
    if let Some(s) = &social {
        api::save_social(&path, s).map_err(|e| format!("{e:#}"))?;
    }
    Ok(json!({ "queued": path, "words": words(&body), "social": social }))
}

/// Where the post will live on the site, as the app builds it for the teasers: its `slug`
/// field, or its file name.
fn post_url(cfg: &ProjectConfig, lang: &str, path: &str) -> String {
    let name = Path::new(path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    cfg.address(lang, &name, &std::fs::read_to_string(path).unwrap_or_default())
}

fn update_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let path = draft_path(&project, &cfg, &lang, &name, linkedin(args)?)?;
    let current = std::fs::read_to_string(&path).map_err(|_| format!("no draft {name} in {lang}"))?;
    let updated = match (args["content"].as_str(), args["body"].as_str()) {
        (Some(c), _) => {
            if frontmatter::split(c).is_none() {
                return Err("content is the whole file, front matter (--- or +++) first".into());
            }
            c.to_string()
        }
        (None, Some(b)) => match split_front_matter(&current) {
            Some(front) => format!("{front}\n\n{}", b.trim_start()),
            None => b.to_string(),
        },
        (None, None) => return Err("give body or content".into()),
    };
    std::fs::write(&path, &updated).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "updated": path, "words": words(&updated) }))
}

/// The front matter, fence lines included, of a post that has one.
fn split_front_matter(content: &str) -> Option<&str> {
    let parts = frontmatter::split(content)?;
    let end = content.len() - parts.body.len();
    Some(content[..end].trim_end_matches(['\n', '\r']))
}

async fn set_social(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let path = folder(&project, &cfg, &lang, false, published(args)?).join(&name);
    if !path.is_file() {
        return Err(format!("no post {name} there ({})", path.display()));
    }
    let path = path.to_string_lossy().into_owned();
    let social = if args["generate"].as_bool().unwrap_or(false) {
        let fields = frontmatter::fields(&std::fs::read_to_string(&path).unwrap_or_default());
        let field = |key: &str| frontmatter::text(&fields, key).unwrap_or_default();
        let (app, key) = model()?;
        let url = post_url(&cfg, &lang, &path);
        api::generate_social_posts(&field("title"), &field("description"), &lang, &url, &app.provider, &key)
            .await
            .map_err(|e| format!("{e:#}"))?
    } else {
        let before: SocialPosts = std::fs::read_to_string(Path::new(&path).with_extension("social.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        SocialPosts {
            linkedin: args["linkedin"].as_str().map_or(before.linkedin, String::from),
            twitter: args["twitter"].as_str().map_or(before.twitter, String::from),
        }
    };
    if social.twitter.chars().count() > 280 {
        return Err(format!("the X teaser is {} characters; X takes 280", social.twitter.chars().count()));
    }
    api::save_social(&path, &social).map_err(|e| format!("{e:#}"))?;
    Ok(json!({ "post": path, "social": social }))
}

fn publish(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let linkedin = linkedin(args)?;
    draft_path(&project, &cfg, &lang, &name, linkedin)?;
    if linkedin {
        api::publish_linkedin_post(&project, &cfg, &lang, &name).map_err(|e| format!("{e:#}"))?;
        return Ok(json!({ "published": folder(&project, &cfg, &lang, true, true).join(&name) }));
    }
    if !args["force"].as_bool().unwrap_or(false) {
        let post = load_posts(&project).blog(&lang).queue.into_iter().find(|p| p.filename == name);
        if let Some(post) = post {
            let issues = crate::checks::check(&post, &project, &cfg);
            let problems: Vec<&str> = crate::checks::problems(&issues).iter().map(|i| i.message.as_str()).collect();
            if !problems.is_empty() {
                return Err(format!("not published, its checks found problems: {} Fix them, or pass force: true if the user wants it out anyway.", problems.join(" ")));
            }
        }
    }
    let done = api::publish_post(&project, &lang, &name, &cfg).map_err(|e| format!("{e:#}"))?;
    let mut v = json!({ "published": done.path, "url": cfg.post_url(&lang, &name) });
    v["note"] = match &done.command {
        None => json!("The site has no publish command; the post goes live with the site's next build and deploy."),
        Some(run) if run.ok => json!(format!("{} ran; the post goes live with the site's next deploy.", run.command)),
        Some(run) => json!(format!("The post is published, but {} failed: {}", run.command, run.output)),
    };
    Ok(v)
}

fn schedule(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    draft_path(&project, &cfg, &lang, &name, false)?;
    let stage = match args["stage"].as_str() {
        Some(s) => Some(Stage::from_key(s).ok_or(format!("stage is idea, draft or review, not {s}"))?),
        None => None,
    };
    let publish_at = match args["publish_at"].as_str().map(str::trim) {
        Some("") => Some(None),
        Some(s) => {
            let when = plan::parse_when(s).ok_or(format!("publish_at is a local date and time like 2026-11-02T09:00, not {s}"))?;
            Some(Some(plan::format_when(when)))
        }
        None => None,
    };
    if stage.is_none() && publish_at.is_none() {
        return Err("give stage, publish_at, or both".into());
    }
    plan::update(&project, &lang, &name, |entry| {
        if let Some(stage) = stage {
            entry.stage = stage;
        }
        if let Some(when) = publish_at {
            entry.publish_at = when;
        }
    })
    .map_err(|e| e.to_string())?;
    let entry = plan::Plan::load(&project).get(&lang, &name);
    Ok(json!({ "post": name, "stage": entry.stage.key(), "publish_at": entry.publish_at }))
}

async fn crosspost(args: &Value) -> Result<Value, String> {
    use crate::crosspost::{Article, Target};
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let target = text(args, "target")?;
    let target = Target::from_key(&target).ok_or(format!("target is devto, hashnode, wordpress or ghost, not {target}"))?;
    let path = folder(&project, &cfg, &lang, false, true).join(&name);
    let content = std::fs::read_to_string(&path).map_err(|_| format!("no published post {name} in {lang}"))?;
    if frontmatter::flag(&frontmatter::fields(&content), "draft") == Some(true) {
        return Err(format!("{name} is a draft; publish it first"));
    }
    let article = Article::of(&content, &cfg.site_url, &cfg.address(&lang, &name, &content));
    let accounts = AppConfig::load().crosspost;
    let copy = crate::crosspost::crosspost(&project, &accounts, target, &lang, &name, &article)
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(json!({ "target": target.label(), "url": copy.url, "as_draft": accounts.as_draft, "canonical_url": article.canonical_url }))
}

fn delete(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let linkedin = linkedin(args)?;
    draft_path(&project, &cfg, &lang, &name, linkedin)?;
    if linkedin {
        api::delete_linkedin_queued(&project, &cfg, &lang, &name)
    } else {
        api::delete_queued(&project, &lang, &name, &cfg)
    }
    .map_err(|e| format!("{e:#}"))?;
    Ok(json!({ "deleted": name }))
}

async fn queue_linkedin(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let topic = text(args, "topic")?;
    let body = match args["body"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        Some(b) => b.to_string(),
        None if args["generate"].as_bool().unwrap_or(false) => {
            let (app, key) = model()?;
            let angle = args["angle"].as_str().unwrap_or("");
            api::generate_linkedin_post(&topic, angle, &lang, &app.provider, &key, &cfg).await.map_err(|e| format!("{e:#}"))?
        }
        None => return Err("give body, or generate: true (with an angle) to have PublishKit's model write it".into()),
    };
    let path = api::save_linkedin_to_queue(&project, &cfg, &topic, &lang, &body).map_err(|e| format!("{e:#}"))?;
    Ok(json!({ "queued": path, "words": words(&body) }))
}

// ── The window ───────────────────────────────────────────────────────────────────────────

/// This executable, as a client should start it.
fn command() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "publishkit".into())
}

/// The `mcpServers` entry for Claude Desktop's config file. Built with serde so a Windows
/// path's backslashes arrive escaped.
pub fn desktop_config(command: &str) -> String {
    let config = json!({ "mcpServers": { "publishkit": { "command": command, "args": ["mcp"] } } });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

/// The one line that adds it to Claude Code, for every project.
pub fn code_command(command: &str) -> String {
    format!("claude mcp add publishkit --scope user -- \"{command}\" mcp")
}

fn desktop_config_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "%APPDATA%\\Claude\\claude_desktop_config.json"
    } else if cfg!(target_os = "macos") {
        "~/Library/Application Support/Claude/claude_desktop_config.json"
    } else {
        "~/.config/Claude/claude_desktop_config.json"
    }
}

#[component]
pub fn McpModal(show: Signal<bool>) -> Element {
    let command = command();
    let desktop = desktop_config(&command);
    let code = code_command(&command);
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| show.set(false) }
        div { class: "modal mcp-modal",
            div { class: "modal-header",
                h2 { "Use PublishKit from Claude" }
                button { class: "btn-icon", onclick: move |_| show.set(false), "✕" }
            }
            div { class: "modal-body",
                p { class: "settings-hint mcp-lead",
                    "Ask Claude to write, edit and translate your drafts, prepare their LinkedIn and X teasers, and \
                     publish when you say so. It works on the same posts as this window, and the lists here follow."
                }
                div { class: "settings-field",
                    label { "1 · Claude Desktop" }
                    p { class: "settings-hint",
                        "Settings → Developer → Edit Config, paste this in and save, then quit Claude and open it again. \
                         If the file already has an mcpServers section, add just the publishkit entry to it."
                    }
                    Snippet { text: desktop.clone() }
                    p { class: "settings-hint mcp-path", "The file: {desktop_config_path()}" }
                }
                div { class: "settings-field",
                    label { "2 · Claude Code" }
                    p { class: "settings-hint", "Run this once in a terminal:" }
                    Snippet { text: code.clone() }
                }
                div { class: "settings-field",
                    label { "3 · Try it" }
                    p { class: "settings-hint",
                        "Ask: “Write next week's post about remote interviews, in French and English, and queue both.”"
                    }
                }
                p { class: "settings-hint mcp-note",
                    "Claude in the browser (claude.ai) can’t use this: it only connects to servers on the internet, and \
                     PublishKit runs on your computer. Any other app that speaks MCP can, with the same command: "
                    code { "{command} mcp" }
                }
            }
        }
    }
}

/// A block of text to paste somewhere else, with its own Copy button.
#[component]
fn Snippet(text: String) -> Element {
    let mut copied = use_signal(|| false);
    let to_copy = text.clone();
    rsx! {
        div { class: "mcp-snippet-wrap",
            pre { class: "mcp-snippet", "{text}" }
            button {
                class: "btn-small mcp-copy",
                onclick: move |_| {
                    crate::screens::copy_to_clipboard(&to_copy);
                    copied.set(true);
                },
                if copied() { "Copied ✓" } else { "Copy" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_tool_has_a_unique_name_and_an_object_schema() {
        let tools = tools();
        let tools = tools.as_array().unwrap();
        let names: HashSet<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), tools.len());
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
            for required in t["inputSchema"]["required"].as_array().into_iter().flatten() {
                let key = required.as_str().unwrap();
                assert!(t["inputSchema"]["properties"].get(key).is_some(), "{} requires {key} undescribed", t["name"]);
            }
        }
    }

    #[test]
    fn a_file_name_never_leaves_its_folder() {
        assert_eq!(filename(&json!({ "filename": "post.md" })).unwrap(), "post.md");
        assert_eq!(filename(&json!({ "filename": "post/index.mdx" })).unwrap(), "post/index.mdx");
        for bad in ["../post.md", "a/b.md", ".hidden.md", "post.txt", "..\\x.md", "../index.md", "a/b/index.md", "/index.md"] {
            assert!(filename(&json!({ "filename": bad })).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_body_keeps_the_front_matter() {
        let post = "---\ntitle: \"A\"\nstatus: \"draft\"\n---\n\n# A\n\nOld.";
        assert_eq!(split_front_matter(post), Some("---\ntitle: \"A\"\nstatus: \"draft\"\n---"));
        assert_eq!(split_front_matter("no front matter"), None);
        assert_eq!(split_front_matter("+++\ntitle = \"A\"\n+++\nBody"), Some("+++\ntitle = \"A\"\n+++"));
    }

    #[test]
    fn the_desktop_entry_starts_this_app_in_mcp_mode() {
        let config: Value = serde_json::from_str(&desktop_config("/Applications/PublishKit.app/Contents/MacOS/publishkit")).unwrap();
        assert_eq!(config["mcpServers"]["publishkit"]["command"], "/Applications/PublishKit.app/Contents/MacOS/publishkit");
        assert_eq!(config["mcpServers"]["publishkit"]["args"], json!(["mcp"]));
        assert_eq!(code_command("/A B/publishkit"), "claude mcp add publishkit --scope user -- \"/A B/publishkit\" mcp");
    }
}

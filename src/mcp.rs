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
use crate::state::{AppConfig, LinkedInPost, Post, ProjectConfig, SocialPosts, load_posts};
use dioxus::prelude::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// The newest protocol revision this server speaks. A client asking for an older one gets its
/// own back; everything used here exists in all of them.
const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "PublishKit manages a website's blog as markdown files: drafts wait in a queue, \
per language, until they are published, and LinkedIn posts the same way. Start with project, which gives the \
site, its languages and how many posts each has; tools default to the project open in PublishKit. list_posts \
and get_post read posts. queue_post adds a draft: write its body yourself in the site's voice (project gives \
the site's own writing brief), or pass generate: true to have PublishKit's configured model write it. \
update_post edits a queued draft; set_social sets the LinkedIn and X teasers of a post. publish_post makes a \
draft live: it moves into the blog, is dated today, and the site's index is rebuilt, so ask the user before \
publishing, and before delete_post. queue_linkedin adds a standalone LinkedIn post.";

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
    let filename = json!({ "type": "string", "description": "The post's file name, e.g. my-post.md, as list_posts gives it." });
    let kind = json!({ "type": "string", "enum": ["blog", "linkedin"], "description": "Default blog." });
    let place = json!({ "type": "string", "enum": ["queue", "published"], "description": "Default queue." });
    json!([
        {
            "name": "project",
            "description": "The site: its name, URL, author, default tags, its writing brief (the system prompt \
                            PublishKit writes with), its languages, and how many drafts and published posts each \
                            has, blog and LinkedIn. Also the model PublishKit generates with.",
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
            "description": "Adds a blog draft to a language's queue, with the site's author and default tags. The \
                            file starts with the title as a heading, so don't repeat it in the body. Give body, or \
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
            "description": "Changes a queued draft (blog or LinkedIn): body replaces what follows the front matter, \
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
            "description": "Publishes a queued draft: moves it into the blog (or LinkedIn's published folder), dates it \
                            today, and for a blog post rebuilds the site's index with its generate-blog-data script. \
                            Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "lang": lang, "filename": filename, "kind": kind },
                "required": ["lang", "filename"],
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

/// A file name in a posts folder: a plain `.md` name, never a path out of it.
fn filename(args: &Value) -> Result<String, String> {
    let name = text(args, "filename")?;
    if name.contains(['/', '\\']) || name.starts_with('.') || !name.ends_with(".md") {
        return Err(format!("{name} is not a post's file name (like my-post.md)"));
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
            "PublishKit has no {} key: set one in its Settings, or write the text yourself and pass it instead",
            cfg.provider.label()
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
    for lang in cfg.languages() {
        let (queue, blog, li_queue, li_published) = match lang.as_str() {
            "fr" => (posts.queue_fr.len(), posts.published_fr.len(), posts.linkedin_queue_fr.len(), posts.linkedin_published_fr.len()),
            "en" => (posts.queue_en.len(), posts.published_en.len(), posts.linkedin_queue_en.len(), posts.linkedin_published_en.len()),
            _ => {
                let count = |linkedin, published| {
                    std::fs::read_dir(folder(&project, &cfg, &lang, linkedin, published))
                        .map(|d| d.filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "md")).count())
                        .unwrap_or(0)
                };
                (count(false, false), count(false, true), count(true, false), count(true, true))
            }
        };
        counts.insert(
            lang,
            json!({ "drafts": queue, "published": blog, "linkedin_drafts": li_queue, "linkedin_published": li_published }),
        );
    }
    let app = AppConfig::load();
    Ok(json!({
        "project": project,
        "site_name": cfg.site_name,
        "site_url": cfg.site_url,
        "author": cfg.author,
        "default_tags": cfg.default_tags,
        "writing_brief": cfg.system_prompt,
        "social_teasers": cfg.social_enabled,
        "languages": counts,
        "model": format!("{} ({})", app.provider.label(), app.provider.model()),
        "model_key_set": !app.resolved_api_key().is_empty(),
    }))
}

/// Posts of every kind and place in one language, as `load_posts` reads them for fr and en,
/// and from the folders directly for any other language.
fn posts_in(project: &str, cfg: &ProjectConfig, lang: &str, linkedin: bool, published: bool) -> Vec<Value> {
    let all = load_posts(project);
    match (lang, linkedin, published) {
        ("fr", false, false) => all.queue_fr.iter().map(post_row).collect(),
        ("fr", false, true) => all.published_fr.iter().map(post_row).collect(),
        ("en", false, false) => all.queue_en.iter().map(post_row).collect(),
        ("en", false, true) => all.published_en.iter().map(post_row).collect(),
        ("fr", true, false) => all.linkedin_queue_fr.iter().map(linkedin_row).collect(),
        ("fr", true, true) => all.linkedin_published_fr.iter().map(linkedin_row).collect(),
        ("en", true, false) => all.linkedin_queue_en.iter().map(linkedin_row).collect(),
        ("en", true, true) => all.linkedin_published_en.iter().map(linkedin_row).collect(),
        _ => {
            let dir = folder(project, cfg, lang, linkedin, published);
            let mut names: Vec<String> = std::fs::read_dir(&dir)
                .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            names.retain(|n| n.ends_with(".md"));
            names.sort();
            names.into_iter().map(|n| json!({ "filename": n })).collect()
        }
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
            v.insert("queue".into(), Value::Array(posts_in(&project, &cfg, &lang, linkedin, false)));
        }
        if done {
            v.insert("published".into(), Value::Array(posts_in(&project, &cfg, &lang, linkedin, true)));
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
    Ok(json!({ "path": path, "content": content, "words": words(&content), "social": social }))
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

/// Where the post will live on the site, as the app builds it for the teasers.
fn post_url(cfg: &ProjectConfig, lang: &str, path: &str) -> String {
    let slug = Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    format!("{}/{}/blog/{}", cfg.site_url, lang, slug)
}

fn update_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let path = folder(&project, &cfg, &lang, linkedin(args)?, false).join(&name);
    let current = std::fs::read_to_string(&path).map_err(|_| format!("no draft {name} in the {lang} queue"))?;
    let updated = match (args["content"].as_str(), args["body"].as_str()) {
        (Some(c), _) => {
            if !c.starts_with("---") {
                return Err("content is the whole file, front matter (---) first".into());
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

/// The front matter, `---` lines included, of a post that has one.
fn split_front_matter(content: &str) -> Option<&str> {
    let rest = content.strip_prefix("---")?;
    let end = rest.find("\n---")? + 3 + 4;
    Some(&content[..end])
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
        let post = std::fs::read_to_string(&path).unwrap_or_default();
        let field = |key: &str| {
            post.lines()
                .find_map(|l| l.strip_prefix(&format!("{key}:")))
                .map(|v| v.trim().trim_matches('"').to_string())
                .unwrap_or_default()
        };
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
    if !folder(&project, &cfg, &lang, linkedin, false).join(&name).is_file() {
        return Err(format!("no draft {name} in the {lang} queue"));
    }
    if linkedin {
        api::publish_linkedin_post(&project, &lang, &name)
    } else {
        api::publish_post(&project, &lang, &name, &cfg)
    }
    .map_err(|e| format!("{e:#}"))?;
    let now = folder(&project, &cfg, &lang, linkedin, true).join(&name);
    let mut v = json!({ "published": now });
    if !linkedin {
        v["note"] = json!("The site's index was rebuilt with scripts/generate-blog-data.js; the post goes live with the site's next deploy.");
    }
    Ok(v)
}

fn delete(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let linkedin = linkedin(args)?;
    if !folder(&project, &cfg, &lang, linkedin, false).join(&name).is_file() {
        return Err(format!("no draft {name} in the {lang} queue"));
    }
    if linkedin {
        api::delete_linkedin_queued(&project, &lang, &name)
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
            api::generate_linkedin_post(&topic, angle, &lang, &app.provider, &key).await.map_err(|e| format!("{e:#}"))?
        }
        None => return Err("give body, or generate: true (with an angle) to have PublishKit's model write it".into()),
    };
    let path = api::save_linkedin_to_queue(&project, &topic, &lang, &body).map_err(|e| format!("{e:#}"))?;
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
        for bad in ["../post.md", "a/b.md", ".hidden.md", "post.txt", "..\\x.md"] {
            assert!(filename(&json!({ "filename": bad })).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_body_keeps_the_front_matter() {
        let post = "---\ntitle: \"A\"\nstatus: \"draft\"\n---\n\n# A\n\nOld.";
        assert_eq!(split_front_matter(post), Some("---\ntitle: \"A\"\nstatus: \"draft\"\n---"));
        assert_eq!(split_front_matter("no front matter"), None);
    }

    #[test]
    fn the_desktop_entry_starts_this_app_in_mcp_mode() {
        let config: Value = serde_json::from_str(&desktop_config("/Applications/PublishKit.app/Contents/MacOS/publishkit")).unwrap();
        assert_eq!(config["mcpServers"]["publishkit"]["command"], "/Applications/PublishKit.app/Contents/MacOS/publishkit");
        assert_eq!(config["mcpServers"]["publishkit"]["args"], json!(["mcp"]));
        assert_eq!(code_command("/A B/publishkit"), "claude mcp add publishkit --scope user -- \"/A B/publishkit\" mcp");
    }
}

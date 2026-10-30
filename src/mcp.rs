//! `publishkit mcp` — the blog's posts, served to an MCP client over stdio, and the window that
//! says how to connect one.
//!
//! No window: the same files and the same functions the app uses (`state::load_posts`,
//! `api::save_to_queue`, `api::publish_post`, …), driven from tool calls instead. Posts are the
//! project's markdown files, which the app may have open too; it reloads its lists when they
//! change (see the watcher in `MainLayout`).
//!
//! The client usually writes a post itself; the app's own model (api0, in the region set in
//! its Settings) is only called when a tool is asked to generate.

use crate::api;
use crate::frontmatter;
use crate::plan::{self, Stage};
use crate::state::{load_posts, AppConfig, Channel, Post, ProjectConfig, SocialPost, SocialPosts};
use dioxus::prelude::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
mod modal;
pub use modal::*;
mod tools;
use tools::*;
mod posts;
use posts::*;

/// The newest protocol revision this server speaks. A client asking for an older one gets its
/// own back; everything used here exists in all of them.
const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "PublishKit manages a website's blog as markdown files: drafts wait, per language, \
until they are published; LinkedIn and X posts are kept the same way, each on its own, not tied to a blog post. \
Start with project, which gives the site, the framework it is built with, its languages and how many posts each \
has; tools default to the project open in PublishKit. list_posts and get_post read posts (kind blog, linkedin or \
x). queue_post adds a blog draft: write its body yourself in the site's voice (project gives the site's own \
writing brief), or pass generate: true to have PublishKit's configured model write it. queue_linkedin and \
queue_x add a LinkedIn or X post: the user usually writes these, so save the text they give as it is, and write \
or generate one only when asked; with published_at it is saved as already posted. update_post edits a draft; \
set_social sets the older LinkedIn and X teasers kept with a blog post. publish_post makes a draft live: it is \
dated today (or published_at, for a post that went out earlier) and the site's publish command, if it has one, \
runs, so ask the user before publishing, and before delete_post. schedule_post sets a draft's stage (idea, \
draft, review) and the date and time it is published by itself; ask the user before scheduling. A LinkedIn or \
X post can carry images or one video: media (file paths) on queue_linkedin, queue_x or update_post, posted posts \
included. PublishKit never posts to LinkedIn or X itself: such a draft's schedule is a reminder on the calendar. \
set_date changes the date and time of a post, published ones included.";

pub fn serve() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("publishkit mcp: could not start: {e}");
            std::process::exit(1);
        }
    };
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
            let Some(id) = message.get("id").cloned() else {
                continue;
            };
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
            result(
                id,
                json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
            )
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
        "set_date" => set_date(args),
        "crosspost_post" => crosspost(args).await,
        "queue_linkedin" => queue_social(args, Channel::LinkedIn).await,
        "queue_x" => queue_social(args, Channel::X).await,
        _ => Err(format!("no tool {name}")),
    }
}

// ── Arguments ────────────────────────────────────────────────────────────────────────────

/// A date given as `key`: `2026-10-16`, or with its time, `2026-10-16T09:00`. `None` when
/// it isn't given.
fn when(args: &Value, key: &str) -> Result<Option<plan::When>, String> {
    match args[key].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => plan::When::parse(s).map(Some).ok_or(format!(
            "{key} is a date like 2026-10-16, or with its local time like 2026-10-16T09:00, not {s}"
        )),
    }
}

fn text(args: &Value, key: &str) -> Result<String, String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or(format!("{key} is required"))
}

/// The site's folder: given, or the one open in PublishKit.
fn project_path(args: &Value) -> Result<String, String> {
    let path = match args["project"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
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
        return Err(format!(
            "no language {lang} in this site; it has {}",
            langs.join(", ")
        ));
    }
    Ok(lang)
}

/// A post's name in its posts folder: a plain `.md` or `.mdx` name, or a page bundle's
/// `folder/index.md` — never a path out of the folder.
fn filename(args: &Value) -> Result<String, String> {
    let name = text(args, "filename")?;
    let plain = |n: &str| {
        !n.contains(['/', '\\'])
            && !n.starts_with('.')
            && (n.ends_with(".md") || n.ends_with(".mdx"))
    };
    let ok = match name.split_once('/') {
        Some((folder, file)) => {
            !folder.is_empty()
                && !folder.starts_with('.')
                && !folder.contains(['/', '\\'])
                && crate::state::is_bundle_index(file)
                && !file.contains('/')
        }
        None => plain(&name),
    };
    if !ok {
        return Err(format!(
            "{name} is not a post's file name (like my-post.md, or my-post/index.md for a bundle)"
        ));
    }
    Ok(name)
}

/// `kind`: `None` for the blog, else the network.
fn kind(args: &Value) -> Result<Option<Channel>, String> {
    match args["kind"].as_str().unwrap_or("blog") {
        "blog" => Ok(None),
        other => Channel::from_key(other)
            .map(Some)
            .ok_or(format!("kind is blog, linkedin or x, not {other}")),
    }
}

fn kind_key(kind: Option<Channel>) -> &'static str {
    kind.map_or("blog", Channel::key)
}

/// `media`, when given: absolute paths of files to attach, or names of ones the post has.
fn media(args: &Value) -> Result<Option<Vec<PathBuf>>, String> {
    let Some(given) = args.get("media").filter(|m| !m.is_null()) else {
        return Ok(None);
    };
    let items = given.as_array().ok_or("media is a list of file paths")?;
    let mut paths = Vec::with_capacity(items.len());
    for item in items {
        let s = item.as_str().map(str::trim).unwrap_or("");
        let path = PathBuf::from(s);
        if s.is_empty() || !(path.is_absolute() || path.components().count() == 1) {
            return Err(format!(
                "{s:?} in media is neither an absolute path nor the name of a file the post has"
            ));
        }
        paths.push(path);
    }
    Ok(Some(paths))
}

fn published(args: &Value) -> Result<bool, String> {
    match args["where"].as_str().unwrap_or("queue") {
        "queue" => Ok(false),
        "published" => Ok(true),
        other => Err(format!("where is queue or published, not {other}")),
    }
}

/// The folder a post is in.
fn folder(
    project: &str,
    cfg: &ProjectConfig,
    lang: &str,
    kind: Option<Channel>,
    published: bool,
) -> PathBuf {
    let (queue, done) = match kind {
        Some(channel) => cfg.resolve_social_paths(project, channel, lang),
        None => cfg.resolve_paths(project, lang),
    };
    if published {
        done
    } else {
        queue
    }
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
        "time": p.when.and_then(|w| w.time_label()),
        "status": p.status,
        "tags": p.tags,
        "stage": p.is_draft().then(|| p.stage.key()),
        "publish_at": p.publish_at,
        "words": words(&p.body),
        "has_social": p.social.is_some(),
    })
}

fn social_row(p: &SocialPost) -> Value {
    json!({
        "filename": p.filename,
        "topic": p.topic,
        "date": p.date,
        "time": p.when.and_then(|w| w.time_label()),
        "status": p.status,
        "publish_at": p.publish_at,
        "words": words(&p.body),
        "characters": p.body.trim().chars().count(),
        "media": p.media,
    })
}

// ── Tools ────────────────────────────────────────────────────────────────────────────────

fn project_info(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let posts = load_posts(&project);
    let mut counts = serde_json::Map::new();
    for lang in &posts.languages {
        let blog = posts.blog(lang);
        let mut count = json!({
            "drafts": blog.queue.len(),
            "published": blog.published.len(),
        });
        for channel in Channel::ALL {
            let shelf = posts.social(channel, lang);
            count[format!("{}_drafts", channel.key())] = json!(shelf.queue.len());
            count[format!("{}_published", channel.key())] = json!(shelf.published.len());
        }
        counts.insert(lang.clone(), count);
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
        "model": app.provider.label(),
        "model_key_set": !app.resolved_api_key().is_empty(),
    }))
}

/// Posts of one kind and place in one language.
fn posts_in(project: &str, lang: &str, kind: Option<Channel>, published: bool) -> Vec<Value> {
    let all = load_posts(project);
    if let Some(channel) = kind {
        let shelf = all.social(channel, lang);
        let posts = if published {
            &shelf.published
        } else {
            &shelf.queue
        };
        posts.iter().map(social_row).collect()
    } else {
        let shelf = all.blog(lang);
        let posts = if published {
            &shelf.published
        } else {
            &shelf.queue
        };
        posts.iter().map(post_row).collect()
    }
}

/// A blog draft's path, refused when there is no such draft: where drafts sit with published
/// posts, the file being there isn't enough.
fn draft_path(
    project: &str,
    cfg: &ProjectConfig,
    lang: &str,
    name: &str,
    kind: Option<Channel>,
) -> Result<PathBuf, String> {
    let path = folder(project, cfg, lang, kind, false).join(name);
    let is_draft = |p: &Path| {
        kind.is_some()
            || cfg.drafts == crate::state::Drafts::Folder
            || std::fs::read_to_string(p)
                .is_ok_and(|c| frontmatter::flag(&frontmatter::fields(&c), "draft") == Some(true))
    };
    if path.is_file() && is_draft(&path) {
        Ok(path)
    } else {
        Err(format!("no draft {name} in {lang}"))
    }
}

/// The front matter, fence lines included, of a post that has one.
fn split_front_matter(content: &str) -> Option<&str> {
    let parts = frontmatter::split(content)?;
    let end = content.len() - parts.body.len();
    Some(content[..end].trim_end_matches(['\n', '\r']))
}

// ── The window ───────────────────────────────────────────────────────────────────────────

/// This executable, as a client should start it.
fn command() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "publishkit".into())
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

/// The one line that adds it to Codex. Its CLI, IDE extension and app share
/// one list of servers, each one there for every project.
pub fn codex_command(command: &str) -> String {
    format!("codex mcp add publishkit -- \"{command}\" mcp")
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

#[cfg(test)]
mod tests;

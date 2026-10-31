//! The tools that read, write, publish and schedule posts.

use super::*;

pub(super) fn list_posts(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let kind = kind(args)?;
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
    let limit = args["limit"].as_u64().map_or(10, |n| n as usize);
    let offset = args["offset"].as_u64().map_or(0, |n| n as usize);
    let mut out = serde_json::Map::new();
    for lang in langs {
        let mut v = serde_json::Map::new();
        if queue {
            v.insert(
                "queue".into(),
                Value::Array(posts_in(&project, &lang, kind, false)),
            );
        }
        if done {
            // Newest first, a page at a time and without descriptions: a site's
            // whole archive in every language would fill a client's context.
            let all = posts_in(&project, &lang, kind, true);
            let page: Vec<Value> = all
                .iter()
                .skip(offset)
                .take(limit)
                .map(|p| {
                    let mut p = p.clone();
                    if let Some(o) = p.as_object_mut() {
                        o.remove("description");
                    }
                    p
                })
                .collect();
            let older = all.len().saturating_sub(offset + page.len());
            v.insert("published".into(), Value::Array(page));
            v.insert("published_total".into(), json!(all.len()));
            if older > 0 {
                v.insert(
                    "published_older".into(),
                    json!(format!("{older} more; pass offset {}", offset + limit)),
                );
            }
        }
        out.insert(lang, Value::Object(v));
    }
    Ok(json!({ "kind": kind_key(kind), "posts": out }))
}

pub(super) fn get_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let path = folder(&project, &cfg, &lang, kind(args)?, published(args)?).join(&name);
    let content = std::fs::read_to_string(&path)
        .map_err(|_| format!("no post {name} there ({})", path.display()))?;
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
    Ok(
        json!({ "path": path, "front_matter": front_matter, "content": content, "words": words(&content), "social": social, "checks": checks }),
    )
}

pub(super) async fn queue_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let title = text(args, "title")?;
    let summary = text(args, "summary")?;
    let body = match args["body"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(b) => b.to_string(),
        None if args["generate"].as_bool().unwrap_or(false) => {
            let (app, key) = model()?;
            api::generate_post(&title, &summary, &lang, &app.provider, &key, &cfg)
                .await
                .map_err(|e| format!("{e:#}"))?
        }
        None => {
            return Err("give body, or generate: true to have PublishKit's model write it".into())
        }
    };
    let path = api::save_to_queue(&project, &title, &summary, &lang, &body, &cfg)
        .map_err(|e| format!("{e:#}"))?;
    let social = if let Some(s) = args.get("social").filter(|s| s.is_object()) {
        Some(SocialPosts {
            linkedin: s["linkedin"].as_str().unwrap_or("").to_string(),
            twitter: s["twitter"].as_str().unwrap_or("").to_string(),
        })
    } else if args["generate_social"].as_bool().unwrap_or(false) {
        let (app, key) = model()?;
        let url = post_url(&cfg, &lang, &path);
        Some(
            api::generate_social_posts(&title, &summary, &lang, &url, &app.provider, &key)
                .await
                .map_err(|e| format!("{e:#}"))?,
        )
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
pub(super) fn post_url(cfg: &ProjectConfig, lang: &str, path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    cfg.address(
        lang,
        &name,
        &std::fs::read_to_string(path).unwrap_or_default(),
    )
}

pub(super) fn update_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let kind = kind(args)?;
    let media = media(args)?;
    let text_given = args["content"].is_string() || args["body"].is_string();
    let path = if published(args)? {
        // Posted already: only its media, so a post made on the network is recorded complete.
        let Some(channel) = kind else {
            return Err("published blog posts aren't edited here".into());
        };
        if text_given || media.is_none() {
            return Err(format!(
                "a posted {} post takes media only here",
                channel.label()
            ));
        }
        let path = folder(&project, &cfg, &lang, kind, true).join(&name);
        if !path.is_file() {
            return Err(format!(
                "no posted {} post {name} in {lang}",
                channel.label()
            ));
        }
        path
    } else {
        draft_path(&project, &cfg, &lang, &name, kind)?
    };
    let channel = match (kind, &media) {
        (None, Some(_)) => {
            return Err("media is for LinkedIn and X posts (kind linkedin or x)".into())
        }
        (channel, _) => channel,
    };
    let current =
        std::fs::read_to_string(&path).map_err(|_| format!("no draft {name} in {lang}"))?;
    let updated = match (args["content"].as_str(), args["body"].as_str()) {
        (Some(c), _) => {
            if frontmatter::split(c).is_none() {
                return Err("content is the whole file, front matter (--- or +++) first".into());
            }
            Some(c.to_string())
        }
        (None, Some(b)) => Some(match split_front_matter(&current) {
            Some(front) => format!("{front}\n\n{}", b.trim_start()),
            None => b.to_string(),
        }),
        (None, None) if media.is_some() => None,
        (None, None) => return Err("give body, content or media".into()),
    };
    if let Some(updated) = &updated {
        std::fs::write(&path, updated).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let mut v = json!({ "updated": path });
    if let (Some(sources), Some(channel)) = (media, channel) {
        v["media"] = json!(api::set_media(&path, channel, &sources).map_err(|e| format!("{e:#}"))?);
    }
    v["words"] = json!(words(&std::fs::read_to_string(&path).unwrap_or_default()));
    Ok(v)
}

pub(super) async fn set_social(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let path = folder(&project, &cfg, &lang, None, published(args)?).join(&name);
    if !path.is_file() {
        return Err(format!("no post {name} there ({})", path.display()));
    }
    let path = path.to_string_lossy().into_owned();
    let social = if args["generate"].as_bool().unwrap_or(false) {
        let fields = frontmatter::fields(&std::fs::read_to_string(&path).unwrap_or_default());
        let field = |key: &str| frontmatter::text(&fields, key).unwrap_or_default();
        let (app, key) = model()?;
        let url = post_url(&cfg, &lang, &path);
        api::generate_social_posts(
            &field("title"),
            &field("description"),
            &lang,
            &url,
            &app.provider,
            &key,
        )
        .await
        .map_err(|e| format!("{e:#}"))?
    } else {
        let before: SocialPosts =
            std::fs::read_to_string(Path::new(&path).with_extension("social.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
        SocialPosts {
            linkedin: args["linkedin"]
                .as_str()
                .map_or(before.linkedin, String::from),
            twitter: args["twitter"]
                .as_str()
                .map_or(before.twitter, String::from),
        }
    };
    if social.twitter.chars().count() > 280 {
        return Err(format!(
            "the X teaser is {} characters; X takes 280",
            social.twitter.chars().count()
        ));
    }
    api::save_social(&path, &social).map_err(|e| format!("{e:#}"))?;
    Ok(json!({ "post": path, "social": social }))
}

pub(super) fn publish(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let kind = kind(args)?;
    let at = when(args, "published_at")?;
    draft_path(&project, &cfg, &lang, &name, kind)?;
    if let Some(channel) = kind {
        api::publish_social_post(&project, &cfg, channel, &lang, &name, at)
            .map_err(|e| format!("{e:#}"))?;
        return Ok(json!({ "published": folder(&project, &cfg, &lang, kind, true).join(&name) }));
    }
    if !args["force"].as_bool().unwrap_or(false) {
        let post = load_posts(&project)
            .blog(&lang)
            .queue
            .into_iter()
            .find(|p| p.filename == name);
        if let Some(post) = post {
            let issues = crate::checks::check(&post, &project, &cfg);
            let problems: Vec<&str> = crate::checks::problems(&issues)
                .iter()
                .map(|i| i.message.as_str())
                .collect();
            if !problems.is_empty() {
                return Err(format!("not published, its checks found problems: {} Fix them, or pass force: true if the user wants it out anyway.", problems.join(" ")));
            }
        }
    }
    let done = api::publish_post(&project, &lang, &name, &cfg, at).map_err(|e| format!("{e:#}"))?;
    let mut v = json!({ "published": done.path, "url": cfg.post_url(&lang, &name) });
    v["note"] = match &done.command {
        None => json!("The site has no publish command; the post goes live with the site's next build and deploy."),
        Some(run) if run.ok => json!(format!("{} ran; the post goes live with the site's next deploy.", run.command)),
        Some(run) => json!(format!("The post is published, but {} failed: {}", run.command, run.output)),
    };
    Ok(v)
}

pub(super) fn schedule(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    if let Some(channel) = kind(args)? {
        return schedule_social(args, &project, &cfg, channel, &lang, &name);
    }
    draft_path(&project, &cfg, &lang, &name, None)?;
    let stage = match args["stage"].as_str() {
        Some(s) => {
            Some(Stage::from_key(s).ok_or(format!("stage is idea, draft or review, not {s}"))?)
        }
        None => None,
    };
    let publish_at = match args["publish_at"].as_str().map(str::trim) {
        Some("") => Some(None),
        Some(s) => {
            let when = plan::parse_when(s).ok_or(format!(
                "publish_at is a local date and time like 2026-11-02T09:00, not {s}"
            ))?;
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

/// A LinkedIn or X draft's planned date: on the calendar, never posted by PublishKit.
fn schedule_social(
    args: &Value,
    project: &str,
    cfg: &ProjectConfig,
    channel: Channel,
    lang: &str,
    name: &str,
) -> Result<Value, String> {
    draft_path(project, cfg, lang, name, Some(channel))?;
    if args.get("stage").is_some_and(|s| !s.is_null()) {
        return Err(format!(
            "{} drafts have no stage; give publish_at only",
            channel.label()
        ));
    }
    let publish_at = match args["publish_at"].as_str().map(str::trim) {
        None => return Err("give publish_at, or an empty string to unschedule".into()),
        Some("") => None,
        Some(s) => Some(plan::format_when(plan::parse_when(s).ok_or(format!(
            "publish_at is a local date and time like 2026-11-02T09:00, not {s}"
        ))?)),
    };
    plan::plan_social(project, channel, lang, name, publish_at).map_err(|e| e.to_string())?;
    Ok(json!({
        "post": name,
        "publish_at": plan::social_at(project, channel, lang, name),
        "note": format!(
            "PublishKit can't post to {0}: the date puts the draft on the calendar. Once it is \
             posted, publish_post with kind {1} records it.",
            channel.label(),
            channel.key()
        ),
    }))
}

/// Sets the date, and time, of a post, published or queued.
pub(super) fn set_date(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let kind = kind(args)?;
    let at = when(args, "date")?.ok_or("date is required")?;
    let published = match args["where"].as_str().unwrap_or("published") {
        "queue" => false,
        "published" => true,
        other => return Err(format!("where is queue or published, not {other}")),
    };
    let path = folder(&project, &cfg, &lang, kind, published).join(&name);
    if !path.is_file() {
        return Err(format!("no post {name} there ({})", path.display()));
    }
    api::set_date(&path, kind.is_some(), at, cfg.front_matter).map_err(|e| format!("{e:#}"))?;
    let date = frontmatter::text(
        &frontmatter::fields(&std::fs::read_to_string(&path).unwrap_or_default()),
        "date",
    );
    Ok(json!({ "post": path, "date": date }))
}

pub(super) async fn crosspost(args: &Value) -> Result<Value, String> {
    use crate::crosspost::{Article, Target};
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let target = text(args, "target")?;
    let target = Target::from_key(&target).ok_or(format!(
        "target is devto, hashnode, wordpress or ghost, not {target}"
    ))?;
    let path = folder(&project, &cfg, &lang, None, true).join(&name);
    let content = std::fs::read_to_string(&path)
        .map_err(|_| format!("no published post {name} in {lang}"))?;
    if frontmatter::flag(&frontmatter::fields(&content), "draft") == Some(true) {
        return Err(format!("{name} is a draft; publish it first"));
    }
    let article = Article::of(
        &content,
        &cfg.site_url,
        &cfg.address(&lang, &name, &content),
    );
    let accounts = AppConfig::load().crosspost;
    let copy = crate::crosspost::crosspost(&project, &accounts, target, &lang, &name, &article)
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(
        json!({ "target": target.label(), "url": copy.url, "as_draft": accounts.as_draft, "canonical_url": article.canonical_url }),
    )
}

pub(super) fn delete(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let kind = kind(args)?;
    draft_path(&project, &cfg, &lang, &name, kind)?;
    if let Some(channel) = kind {
        api::delete_social_queued(&project, &cfg, channel, &lang, &name)
    } else {
        api::delete_queued(&project, &lang, &name, &cfg)
    }
    .map_err(|e| format!("{e:#}"))?;
    Ok(json!({ "deleted": name }))
}

/// `queue_linkedin` and `queue_x`: a post for the network, as the user wrote it, or written by
/// PublishKit's model when asked.
pub(super) async fn queue_social(args: &Value, channel: Channel) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let given_topic = args["topic"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let body =
        match args["body"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(b) => b.to_string(),
            None if args["generate"].as_bool().unwrap_or(false) => {
                let topic = given_topic.ok_or("generate needs a topic")?;
                let (app, key) = model()?;
                let angle = args["angle"].as_str().unwrap_or("");
                let link = args["link"].as_str().unwrap_or("");
                api::generate_social_post(
                    channel,
                    topic,
                    angle,
                    link,
                    &lang,
                    &app.provider,
                    &key,
                    &cfg,
                )
                .await
                .map_err(|e| format!("{e:#}"))?
            }
            None => return Err(
                "give body, or generate: true (with a topic and an angle) to have PublishKit's \
                 model write it"
                    .into(),
            ),
        };
    let topic = given_topic.map_or_else(|| api::topic_of(&body), String::from);
    let at = when(args, "published_at")?;
    let sources = media(args)?.unwrap_or_default();
    let path = api::save_social_to_queue(&project, &cfg, channel, &topic, &lang, &body)
        .map_err(|e| format!("{e:#}"))?;
    let attached = match api::set_media(Path::new(&path), channel, &sources) {
        Ok(names) => names,
        Err(e) => {
            // Nothing half-saved: the draft goes with the media it couldn't take.
            let _ = std::fs::remove_file(&path);
            return Err(format!("{e:#}"));
        }
    };
    let characters = body.chars().count();
    let mut v = if let Some(at) = at {
        // Posted already: saved straight among the posted ones, on its date.
        let name = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        api::publish_social_post(&project, &cfg, channel, &lang, &name, Some(at))
            .map_err(|e| format!("{e:#}"))?;
        let posted = folder(&project, &cfg, &lang, Some(channel), true).join(&name);
        json!({ "published": posted, "date": at.social_date() })
    } else {
        json!({ "queued": path })
    };
    v["words"] = json!(words(&body));
    v["characters"] = json!(characters);
    v["media"] = json!(attached);
    if characters > channel.limit() {
        let premium = if channel == Channel::X {
            ", longer posts only with X Premium"
        } else {
            ""
        };
        v["note"] = json!(format!(
            "{characters} characters: {} takes {}{premium}",
            channel.label(),
            channel.limit()
        ));
    }
    Ok(v)
}

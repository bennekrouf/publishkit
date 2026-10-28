//! The tools that read, write, publish and schedule posts.

use super::*;

pub(super) fn list_posts(args: &Value) -> Result<Value, String> {
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
            v.insert(
                "queue".into(),
                Value::Array(posts_in(&project, &lang, linkedin, false)),
            );
        }
        if done {
            v.insert(
                "published".into(),
                Value::Array(posts_in(&project, &lang, linkedin, true)),
            );
        }
        out.insert(lang, Value::Object(v));
    }
    Ok(json!({ "kind": if linkedin { "linkedin" } else { "blog" }, "posts": out }))
}

pub(super) fn get_post(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let path = folder(&project, &cfg, &lang, linkedin(args)?, published(args)?).join(&name);
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
    let path = draft_path(&project, &cfg, &lang, &name, linkedin(args)?)?;
    let current =
        std::fs::read_to_string(&path).map_err(|_| format!("no draft {name} in {lang}"))?;
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

pub(super) async fn set_social(args: &Value) -> Result<Value, String> {
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
    let linkedin = linkedin(args)?;
    let at = when(args, "published_at")?;
    draft_path(&project, &cfg, &lang, &name, linkedin)?;
    if linkedin {
        api::publish_linkedin_post(&project, &cfg, &lang, &name, at)
            .map_err(|e| format!("{e:#}"))?;
        return Ok(json!({ "published": folder(&project, &cfg, &lang, true, true).join(&name) }));
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
    if linkedin(args)? {
        return schedule_linkedin(args, &project, &cfg, &lang, &name);
    }
    draft_path(&project, &cfg, &lang, &name, false)?;
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

/// A LinkedIn draft's planned date: on the calendar, never posted by PublishKit.
fn schedule_linkedin(
    args: &Value,
    project: &str,
    cfg: &ProjectConfig,
    lang: &str,
    name: &str,
) -> Result<Value, String> {
    draft_path(project, cfg, lang, name, true)?;
    if args.get("stage").is_some_and(|s| !s.is_null()) {
        return Err("LinkedIn drafts have no stage; give publish_at only".into());
    }
    let publish_at = match args["publish_at"].as_str().map(str::trim) {
        None => return Err("give publish_at, or an empty string to unschedule".into()),
        Some("") => None,
        Some(s) => Some(plan::format_when(plan::parse_when(s).ok_or(format!(
            "publish_at is a local date and time like 2026-11-02T09:00, not {s}"
        ))?)),
    };
    plan::plan_linkedin(project, lang, name, publish_at).map_err(|e| e.to_string())?;
    Ok(json!({
        "post": name,
        "publish_at": plan::linkedin_at(project, lang, name),
        "note": "PublishKit can't post to LinkedIn: the date puts the draft on the calendar. Once \
                 it is posted, publish_post with kind linkedin records it.",
    }))
}

/// Sets the date, and time, of a post, published or queued.
pub(super) fn set_date(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let name = filename(args)?;
    let linkedin = linkedin(args)?;
    let at = when(args, "date")?.ok_or("date is required")?;
    let published = match args["where"].as_str().unwrap_or("published") {
        "queue" => false,
        "published" => true,
        other => return Err(format!("where is queue or published, not {other}")),
    };
    let path = folder(&project, &cfg, &lang, linkedin, published).join(&name);
    if !path.is_file() {
        return Err(format!("no post {name} there ({})", path.display()));
    }
    api::set_date(&path, linkedin, at, cfg.front_matter).map_err(|e| format!("{e:#}"))?;
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
    let path = folder(&project, &cfg, &lang, false, true).join(&name);
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

pub(super) async fn queue_linkedin(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let topic = text(args, "topic")?;
    let body =
        match args["body"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(b) => b.to_string(),
            None if args["generate"].as_bool().unwrap_or(false) => {
                let (app, key) = model()?;
                let angle = args["angle"].as_str().unwrap_or("");
                api::generate_linkedin_post(&topic, angle, &lang, &app.provider, &key, &cfg)
                    .await
                    .map_err(|e| format!("{e:#}"))?
            }
            None => return Err(
                "give body, or generate: true (with an angle) to have PublishKit's model write it"
                    .into(),
            ),
        };
    let at = when(args, "published_at")?;
    let path = api::save_linkedin_to_queue(&project, &cfg, &topic, &lang, &body)
        .map_err(|e| format!("{e:#}"))?;
    if let Some(at) = at {
        // Posted on LinkedIn already: saved straight among the posted ones, on its date.
        let name = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        api::publish_linkedin_post(&project, &cfg, &lang, &name, Some(at))
            .map_err(|e| format!("{e:#}"))?;
        let posted = folder(&project, &cfg, &lang, true, true).join(&name);
        return Ok(
            json!({ "published": posted, "date": at.linkedin_date(), "words": words(&body) }),
        );
    }
    Ok(json!({ "queued": path, "words": words(&body) }))
}

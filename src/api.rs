use crate::frontmatter::{self, Field, Format};
use crate::plan::When;
use crate::state::{language_name, Channel, Drafts, LlmProvider, ProjectConfig, SocialPosts};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
mod files;
pub use files::*;
mod media;
pub use media::*;

fn write_in(lang: &str) -> String {
    format!("Write entirely in {}.", language_name(lang))
}

/// A LinkedIn or X post written by the model, from what it is about and the points to make;
/// `link`, when there is one, ends up as it is, on a line of its own.
#[allow(clippy::too_many_arguments)]
pub async fn generate_social_post(
    channel: Channel,
    topic: &str,
    angle: &str,
    link: &str,
    lang: &str,
    provider: &LlmProvider,
    api_key: &str,
    project_config: &ProjectConfig,
) -> Result<String> {
    let lang_instruction = write_in(lang);
    let company = if project_config.site_name.is_empty() {
        "the company"
    } else {
        &project_config.site_name
    };

    let system = match channel {
        Channel::LinkedIn => format!(
            "You write LinkedIn posts for {company}'s company page, promoting its products. \
            {lang_instruction} Professional but engaging tone, concrete value proposition, \
            no corporate fluff. Max 3 hashtags. 4-8 short paragraphs/lines, easy to skim."
        ),
        Channel::X => format!(
            "You write posts on X for {company}. {lang_instruction} Punchy and concrete, one \
            idea, no corporate fluff. At most 2 hashtags. At most 270 characters in all, any \
            link included."
        ),
    };
    let user = social_prompt(channel, topic, angle, link);
    let text = call(provider, &system, &user, api_key).await?;
    Ok(place_link(text.trim(), link.trim()))
}

/// What the model is asked for a LinkedIn or X post.
fn social_prompt(channel: Channel, topic: &str, angle: &str, link: &str) -> String {
    let link = link.trim();
    // LinkedIn and X make a bare address clickable, and show no anchor text or markdown.
    let link_rule = if link.is_empty() {
        String::new()
    } else {
        format!(
            "Link: {link}\n\
            Put this link exactly as given, on a line of its own near the end of the post. \
            Plain text only: no markdown link, no angle brackets, no URL shortener, \
            no other link.\n"
        )
    };
    format!(
        "Write a {} post about: \"{topic}\"\n\
        Angle / key points: {angle}\n\
        {link_rule}\n\
        Output ONLY the post text, ready to publish (no markdown headers, no frontmatter).",
        channel.label()
    )
}

/// A written post with `link` as the networks show it: a markdown link or `<…>` around it
/// undone, and the link added on its own line, before closing hashtags, when the model left it out.
pub(crate) fn place_link(text: &str, link: &str) -> String {
    if link.is_empty() {
        return text.to_string();
    }
    let mut text = text.replace(&format!("<{link}>"), link);
    // `[label](link)` becomes the bare link: LinkedIn and X would show the brackets as they are.
    let target = format!("]({link})");
    while let Some(end) = text.find(&target) {
        let Some(start) = text[..end].rfind('[') else {
            break;
        };
        text.replace_range(start..end + target.len(), link);
    }
    if text.contains(link) {
        return text;
    }
    let lines: Vec<&str> = text.lines().collect();
    let tags = lines
        .iter()
        .rev()
        .take_while(|l| {
            let l = l.trim();
            l.is_empty() || l.split_whitespace().all(|w| w.starts_with('#'))
        })
        .count();
    let (body, tail) = lines.split_at(lines.len() - tags);
    let body = body.join("\n");
    let tail = tail.join("\n");
    let tail = tail.trim();
    if tail.is_empty() {
        format!("{}\n\n{link}", body.trim_end())
    } else {
        format!("{}\n\n{link}\n\n{tail}", body.trim_end())
    }
}

/// A post's text cut where its web addresses are, as `(piece, is_link)`: the pieces, in order,
/// make up the text as it is. An address ends at a space, and punctuation closing a sentence
/// after it isn't part of it.
pub fn split_links(text: &str) -> Vec<(&str, bool)> {
    let mut pieces = Vec::new();
    let mut from = 0;
    let mut at = 0;
    while let Some(i) = text[at..].find("http").map(|i| at + i) {
        let rest = &text[i..];
        let scheme = ["https://", "http://"]
            .into_iter()
            .find(|s| rest.starts_with(s));
        // `xhttp://` is a word, not an address.
        let after_word = text[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '/' || c == '@');
        let (Some(scheme), false) = (scheme, after_word) else {
            at = i + 4;
            continue;
        };
        let len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"'))
            .unwrap_or(rest.len());
        let url = trim_url_end(&rest[..len]);
        if url.len() <= scheme.len() {
            at = i + scheme.len();
            continue;
        }
        if from < i {
            pieces.push((&text[from..i], false));
        }
        pieces.push((url, true));
        from = i + url.len();
        at = from;
    }
    if from < text.len() {
        pieces.push((&text[from..], false));
    }
    pieces
}

/// An address without what closes the sentence after it: `.`, `,`, `!`, quotes, and a `)` or
/// `]` it didn't open.
fn trim_url_end(url: &str) -> &str {
    let mut url = url;
    loop {
        let Some(last) = url.chars().next_back() else {
            return url;
        };
        let unopened = |open: char, close: char| {
            last == close && url.matches(close).count() > url.matches(open).count()
        };
        let drop = matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*' | '_')
            || unopened('(', ')')
            || unopened('[', ']');
        if !drop {
            return url;
        }
        url = &url[..url.len() - last.len_utf8()];
    }
}

/// What a post written without a topic is listed as: its first line, cut at a word.
pub fn topic_of(body: &str) -> String {
    let line = body
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= 60 {
        return line.to_string();
    }
    let cut: String = line.chars().take(60).collect();
    match cut.rfind(' ') {
        Some(i) if i > 20 => format!("{}…", &cut[..i]),
        _ => format!("{cut}…"),
    }
}

pub fn save_social_to_queue(
    project_path: &str,
    project_config: &ProjectConfig,
    channel: Channel,
    topic: &str,
    lang: &str,
    body: &str,
) -> Result<String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (queue_dir, _) = project_config.resolve_social_paths(project_path, channel, lang);
    std::fs::create_dir_all(&queue_dir)?;

    let content = frontmatter::build(
        Format::Yaml,
        &[
            ("topic", Field::Text(topic.to_string())),
            ("date", Field::Text(today)),
            ("lang", Field::Text(lang.to_string())),
            ("status", Field::Text("draft".into())),
        ],
        body,
    );
    let slug = Some(slugify(topic))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "post".into());
    let path = free_path(&queue_dir, &slug, "md");
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

/// Replaces a post's text, its front matter kept.
pub fn set_body(path: &Path, body: &str) -> Result<()> {
    let content = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    std::fs::write(path, frontmatter::with_body(&content, body))
        .with_context(|| format!("{}", path.display()))?;
    Ok(())
}

/// Moves a LinkedIn or X draft to the posted ones, dated `at` when it went out earlier (posted
/// on the network first, saved here after), else today.
pub fn publish_social_post(
    project_path: &str,
    project_config: &ProjectConfig,
    channel: Channel,
    lang: &str,
    filename: &str,
    at: Option<When>,
) -> Result<()> {
    let (queue, published) = project_config.resolve_social_paths(project_path, channel, lang);
    let src = queue.join(filename);
    std::fs::create_dir_all(&published)?;
    let dst = published.join(filename);

    let content = std::fs::read_to_string(&src)?;
    let updated = frontmatter::set(
        &content,
        "status",
        &Field::Text("published".into()),
        Format::Yaml,
    );
    let date = at.map_or_else(|| today_like(""), |w| w.social_date());
    let updated = frontmatter::set(&updated, "date", &Field::Text(date), Format::Yaml);

    if dst.exists() {
        bail!("{} is already among the posted ones", dst.display());
    }
    let updated = move_media(&updated, &queue, &published)?;
    std::fs::write(&dst, updated)?;
    std::fs::remove_file(&src)?;
    crate::plan::forget_social(project_path, channel, lang, filename);
    Ok(())
}

/// Sets the date of a post, queued or published, blog or social: written as that kind of
/// post keeps it. A blog post's file name keeps its date prefix: links to it may use it.
pub fn set_date(path: &Path, social: bool, when: When, default: Format) -> Result<()> {
    let content = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    let format = frontmatter::split(&content).map_or(default, |p| p.format);
    let date = if social {
        when.social_date()
    } else {
        let previous =
            frontmatter::text(&frontmatter::fields(&content), "date").unwrap_or_default();
        when.blog_date(&previous)
    };
    let updated = frontmatter::set(&content, "date", &Field::Text(date), format);
    std::fs::write(path, updated)?;
    Ok(())
}

pub fn delete_social_queued(
    project_path: &str,
    project_config: &ProjectConfig,
    channel: Channel,
    lang: &str,
    filename: &str,
) -> Result<()> {
    let (queue, _) = project_config.resolve_social_paths(project_path, channel, lang);
    delete_media(&queue.join(filename));
    std::fs::remove_file(queue.join(filename))?;
    crate::plan::forget_social(project_path, channel, lang, filename);
    Ok(())
}

pub async fn generate_post(
    title: &str,
    summary: &str,
    lang: &str,
    provider: &LlmProvider,
    api_key: &str,
    project_config: &ProjectConfig,
) -> Result<String> {
    let lang_instruction = write_in(lang);

    let system = format!("{} {lang_instruction}", project_config.system_prompt);
    let user = format!(
        "Write a blog post with this title: \"{title}\"\n\
        Summary / angle: {summary}\n\n\
        Output ONLY the markdown body (no frontmatter). Start directly with the first paragraph. \
        Structure: intro, 3-4 sections with ## headings, a conclusion. Aim for ~600-800 words."
    );

    call(provider, &system, &user, api_key).await
}

/// api0's OpenAI-compatible chat endpoint, in the region set in Settings.
async fn call(provider: &LlmProvider, system: &str, user: &str, api_key: &str) -> Result<String> {
    let name = provider.label();
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": provider.model(),
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user}
        ],
        "temperature": 0.75
    });

    let resp = client
        .post(provider.api_url())
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        // So api0 puts the credits down to PublishKit.
        .header("X-Client-App", crate::api0::PRODUCT)
        // So api0 runs the region's model for writing.
        .header("X-Task", crate::api0::TASK)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("{name} request failed"))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let e = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("{name} error {s}: {e}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .with_context(|| format!("{name} parse failed"))?;
    json["choices"][0]["message"]["content"]
        .as_str()
        .map(String::from)
        .with_context(|| format!("Empty {name} response"))
}

pub async fn generate_social_posts(
    title: &str,
    summary: &str,
    lang: &str,
    blog_url: &str,
    provider: &LlmProvider,
    api_key: &str,
) -> Result<SocialPosts> {
    let lang_instruction = write_in(lang);

    let system = format!(
        "You generate social media posts to promote blog articles. \
        {lang_instruction} Be concise, engaging, and professional. \
        Do NOT use hashtags excessively — max 3 for LinkedIn, max 2 for Twitter/X."
    );
    let user = format!(
        "Generate social media teasers for this blog post:\n\
        Title: \"{title}\"\n\
        Summary: {summary}\n\
        URL: {blog_url}\n\n\
        Reply with ONLY a JSON object (no markdown fences, no extra text):\n\
        {{\"linkedin\": \"...\", \"twitter\": \"...\"}}\n\n\
        LinkedIn: 2-3 sentences, professional tone, include the URL at the end.\n\
        Twitter/X: max 250 chars (leave room for the URL), punchy, include the URL."
    );

    let raw = call(provider, &system, &user, api_key).await?;

    // Extract JSON from response (handle potential markdown fences)
    let json_str = raw
        .trim()
        .strip_prefix("```json")
        .or(raw.trim().strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(raw.trim());

    serde_json::from_str::<SocialPosts>(json_str)
        .context("Failed to parse social posts JSON from LLM response")
}

pub fn save_to_queue(
    project_path: &str,
    title: &str,
    summary: &str,
    lang: &str,
    body: &str,
    project_config: &ProjectConfig,
) -> Result<String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (queue_dir, _) = project_config.resolve_paths(project_path, lang);
    std::fs::create_dir_all(&queue_dir)?;

    let mut fields = vec![
        ("title", Field::Text(title.to_string())),
        ("description", Field::Text(summary.to_string())),
        ("date", Field::Text(today)),
    ];
    if !project_config.author.is_empty() {
        fields.push(("author", Field::Text(project_config.author.clone())));
    }
    if !project_config.default_tags.is_empty() {
        fields.push(("tags", Field::List(project_config.default_tags.clone())));
    }
    if project_config.languages().len() > 1 {
        fields.push(("lang", Field::Text(lang.to_string())));
    }
    match project_config.drafts {
        Drafts::Folder => fields.push(("status", Field::Text("draft".into()))),
        Drafts::Flag => fields.push(("draft", Field::Bool(true))),
    }
    let body = if project_config.title_heading {
        format!("# {title}\n\n{body}")
    } else {
        body.to_string()
    };
    let content = frontmatter::build(project_config.front_matter, &fields, &body);
    // Where the file name marks the language: `post.fr.md`, a bundle's `index.fr.md`.
    let extension = match project_config.language_mark(lang) {
        "" => project_config.extension.clone(),
        mark => format!("{mark}.{}", project_config.extension),
    };
    let path = if project_config.bundles {
        free_bundle(&queue_dir, &slugify(title), &extension)
    } else {
        free_path(&queue_dir, &slugify(title), &extension)
    };
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

/// `dir/slug/index.ext` in a folder of its own: `slug-2` and so on when `slug` is taken.
fn free_bundle(dir: &Path, slug: &str, extension: &str) -> PathBuf {
    let slug = if slug.is_empty() { "post" } else { slug };
    let extension = if extension.is_empty() {
        "md"
    } else {
        extension
    };
    let folder = (1..)
        .map(|n| {
            if n == 1 {
                dir.join(slug)
            } else {
                dir.join(format!("{slug}-{n}"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(slug));
    folder.join(format!("index.{extension}"))
}

/// How many posts (languages) a bundle folder holds.
fn bundle_posts(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| crate::state::is_bundle_index(&e.file_name().to_string_lossy()))
                .count()
        })
        .unwrap_or(0)
}

/// A post's own folder, for one that is a page bundle (`my-post/index.md`).
fn bundle_folder(posts_dir: &Path, filename: &str) -> Option<PathBuf> {
    filename
        .split_once('/')
        .map(|(folder, _)| posts_dir.join(folder))
}

/// `dir/slug.ext`, or `dir/slug-2.ext` and so on when that is taken: a new post never
/// replaces one that is there.
fn free_path(dir: &Path, slug: &str, extension: &str) -> PathBuf {
    let slug = if slug.is_empty() { "post" } else { slug };
    let extension = if extension.is_empty() {
        "md"
    } else {
        extension
    };
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{slug}.{extension}"))
            } else {
                dir.join(format!("{slug}-{n}.{extension}"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{slug}.{extension}")))
}

/// The sidecar holding a post's teasers: `my-post.social.json` beside `my-post.md`.
fn social_path(md_path: &Path) -> PathBuf {
    md_path.with_extension("social.json")
}

pub fn save_social(md_path: &str, social: &SocialPosts) -> Result<()> {
    let json = serde_json::to_string_pretty(social)?;
    std::fs::write(social_path(Path::new(md_path)), json)?;
    Ok(())
}

/// Today, written like `previous`: a date and time when it had one, else a plain date.
fn today_like(previous: &str) -> String {
    let now = chrono::Local::now();
    if previous.contains('T') {
        now.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
    } else {
        now.format("%Y-%m-%d").to_string()
    }
}

/// What the site's publish command did.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandRun {
    pub command: String,
    pub ok: bool,
    /// What it printed, its errors first; the start and end of it when it is long.
    pub output: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Published {
    pub path: PathBuf,
    pub command: Option<CommandRun>,
    /// The deploy, when the site deploys after each publish.
    pub deploy: Option<CommandRun>,
}

impl Published {
    /// One line for the person who clicked Publish.
    pub fn summary(&self) -> String {
        let mut name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // A bundle is named by its folder: every one of them holds an index.md.
        if crate::state::is_bundle_index(&name) {
            if let Some(folder) = self.path.parent().and_then(|p| p.file_name()) {
                name = format!("{}/{name}", folder.to_string_lossy());
            }
        }
        let mut line = match &self.command {
            None => format!("Published {name}."),
            Some(run) if run.ok => format!("Published {name}; {} ran.", run.command),
            Some(run) => format!(
                "Published {name}, but {} failed: {}",
                run.command, run.output
            ),
        };
        match &self.deploy {
            Some(run) if run.ok => line.push_str(" Deployed."),
            Some(run) => line.push_str(&format!(" The deploy failed: {}", run.output)),
            None => {}
        }
        line
    }

    /// Whether everything that ran after publishing went well.
    pub fn ok(&self) -> bool {
        [&self.command, &self.deploy]
            .iter()
            .all(|r| r.as_ref().is_none_or(|r| r.ok))
    }
}

/// Publishes a draft. With a drafts folder it moves into the blog's; with `draft: true` it
/// stays where it is and loses the mark. Either way it is dated today, or `at` for a post that
/// went out earlier, then the site's publish command runs. The command failing doesn't undo
/// the publishing: it is reported.
pub fn publish_post(
    project_path: &str,
    lang: &str,
    filename: &str,
    project_config: &ProjectConfig,
    at: Option<When>,
) -> Result<Published> {
    let (queue_path, blog_path) = project_config.resolve_paths(project_path, lang);
    let src = queue_path.join(filename);
    let content = std::fs::read_to_string(&src).with_context(|| format!("{}", src.display()))?;
    let fields = frontmatter::fields(&content);
    let format = frontmatter::split(&content).map_or(project_config.front_matter, |p| p.format);

    let mut updated = content.clone();
    if fields.contains_key("status") || project_config.drafts == Drafts::Folder {
        updated = frontmatter::set(&updated, "status", &Field::Text("published".into()), format);
    }
    if fields.contains_key("draft") {
        updated = frontmatter::set(&updated, "draft", &Field::Bool(false), format);
    }
    let date = frontmatter::text(&fields, "date").unwrap_or_default();
    let date = at.map_or_else(|| today_like(&date), |w| w.blog_date(&date));
    updated = frontmatter::set(&updated, "date", &Field::Text(date), format);

    // A bundle moves as a whole, its images with it.
    if let Some(folder) = bundle_folder(&queue_path, filename) {
        std::fs::write(&src, &updated)?;
        let dst_folder = bundle_folder(&blog_path, filename).unwrap_or_else(|| folder.clone());
        if dst_folder != folder {
            if dst_folder.exists() {
                bail!("{} is already in the blog", dst_folder.display());
            }
            std::fs::create_dir_all(&blog_path)?;
            std::fs::rename(&folder, &dst_folder)
                .with_context(|| format!("{}", folder.display()))?;
        }
        crate::plan::forget(project_path, lang, filename);
        return Ok(after_publish(
            project_path,
            project_config,
            blog_path.join(filename),
        ));
    }

    let mut name = filename.to_string();
    if project_config.date_in_filename && !name.get(..11).is_some_and(crate::state::is_date_prefix)
    {
        let day = at.map_or_else(|| chrono::Local::now().date_naive(), |w| w.day);
        name = format!("{}-{name}", day.format("%Y-%m-%d"));
    }
    std::fs::create_dir_all(&blog_path)?;
    let dst = blog_path.join(&name);
    std::fs::write(&dst, updated)?;
    if dst != src {
        std::fs::remove_file(&src)?;
        if social_path(&src).exists() {
            let _ = std::fs::rename(social_path(&src), social_path(&dst))
                .or_else(|_| std::fs::copy(social_path(&src), social_path(&dst)).map(|_| ()));
            let _ = std::fs::remove_file(social_path(&src));
        }
    }

    crate::plan::forget(project_path, lang, filename);
    Ok(after_publish(project_path, project_config, dst))
}

/// The site's publish command, then its deploy when it deploys after each publish.
fn after_publish(project_path: &str, project_config: &ProjectConfig, path: PathBuf) -> Published {
    let command = run_command(project_path, &project_config.publish_command);
    // No deploy of a site whose index failed to build: it would put the old one online.
    let deploy = if project_config.deploy_after_publish && command.as_ref().is_none_or(|c| c.ok) {
        run_command(project_path, &project_config.deploy_command)
    } else {
        None
    };
    Published {
        path,
        command,
        deploy,
    }
}

/// Runs the site's deploy command; `None` when it has none.
pub fn deploy(project_path: &str, project_config: &ProjectConfig) -> Option<CommandRun> {
    run_command(project_path, &project_config.deploy_command)
}

/// Publishes every draft whose scheduled time has come, earliest first. A scheduled draft that
/// is gone is dropped from the plan.
pub fn publish_due(project_path: &str) -> Vec<(String, Result<Published>)> {
    let project_config = ProjectConfig::load(project_path);
    let plan = crate::plan::Plan::load(project_path);
    let now = chrono::Local::now().naive_local();
    let mut done = vec![];
    for (lang, filename) in plan.due(now) {
        let (queue, _) = project_config.resolve_paths(project_path, &lang);
        if !queue.join(&filename).is_file() {
            crate::plan::forget(project_path, &lang, &filename);
            continue;
        }
        // A draft with problems isn't sent out on its own: it is unscheduled, and said why.
        let post = crate::state::load_posts(project_path)
            .blog(&lang)
            .queue
            .into_iter()
            .find(|p| p.filename == filename);
        if let Some(post) = post {
            let issues = crate::checks::check(&post, project_path, &project_config);
            let problems = crate::checks::problems(&issues);
            if !problems.is_empty() {
                let _ =
                    crate::plan::update(project_path, &lang, &filename, |e| e.publish_at = None);
                let why: Vec<&str> = problems.iter().map(|i| i.message.as_str()).collect();
                done.push((
                    format!("{lang}/{filename}"),
                    Err(anyhow::anyhow!(
                        "held back and unscheduled: {}",
                        why.join(" ")
                    )),
                ));
                continue;
            }
        }
        let result = publish_post(project_path, &lang, &filename, &project_config, None);
        done.push((format!("{lang}/{filename}"), result));
    }
    done
}

fn run_command(project_path: &str, command: &str) -> Option<CommandRun> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let result = shell(command).current_dir(project_path).output();
    Some(match result {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stderr).into_owned();
            if text.trim().is_empty() {
                text = String::from_utf8_lossy(&out.stdout).into_owned();
            }
            CommandRun {
                command: command.to_string(),
                ok: out.status.success(),
                output: shorten(text.trim()),
            }
        }
        Err(e) => CommandRun {
            command: command.to_string(),
            ok: false,
            output: e.to_string(),
        },
    })
}

/// Long output, cut to its start (where most tools say what went wrong) and its end.
fn shorten(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 600 {
        return text.to_string();
    }
    let start: String = chars[..350].iter().collect();
    let end: String = chars[chars.len() - 200..].iter().collect();
    format!("{start} … {end}")
}

/// The person's shell, running `command`. A login shell on macOS and Linux: an app opened from
/// the Dock doesn't have the PATH a terminal has, where node and the like are found.
fn shell(command: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        let mut command = command.to_string();
        if let Some(rest) = command.strip_prefix("node ") {
            command = format!("\"{}\" {rest}", node_command().display());
        }
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &command]);
        c
    }
    #[cfg(not(windows))]
    {
        let sh = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".into());
        let mut c = std::process::Command::new(sh);
        c.args(["-lc", command]);
        c
    }
}

/// The front matter key a post keeps its description under.
pub fn description_key(content: &str) -> &'static str {
    let fields = frontmatter::fields(content);
    ["description", "summary", "excerpt"]
        .into_iter()
        .find(|k| fields.contains_key(*k))
        .unwrap_or("description")
}

#[cfg(test)]
mod tests;

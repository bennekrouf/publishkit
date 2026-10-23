use anyhow::{bail, Context, Result};
use crate::frontmatter::{self, Field, Format};
use crate::state::{language_name, Drafts, LlmProvider, ProjectConfig, SocialPosts};
use std::path::{Path, PathBuf};

fn write_in(lang: &str) -> String {
    format!("Write entirely in {}.", language_name(lang))
}

pub async fn generate_linkedin_post(
    topic: &str,
    angle: &str,
    lang: &str,
    provider: &LlmProvider,
    api_key: &str,
    project_config: &ProjectConfig,
) -> Result<String> {
    let lang_instruction = write_in(lang);
    let company = if project_config.site_name.is_empty() { "the company" } else { &project_config.site_name };

    let system = format!(
        "You write LinkedIn posts for {company}'s company page, promoting its products. \
        {lang_instruction} Professional but engaging tone, concrete value proposition, \
        no corporate fluff. Max 3 hashtags. 4-8 short paragraphs/lines, easy to skim."
    );
    let user = format!(
        "Write a LinkedIn post about: \"{topic}\"\n\
        Angle / key points: {angle}\n\n\
        Output ONLY the post text, ready to publish (no markdown headers, no frontmatter)."
    );

    call(provider, &system, &user, api_key).await
}

pub fn save_linkedin_to_queue(
    project_path: &str,
    project_config: &ProjectConfig,
    topic: &str,
    lang: &str,
    body: &str,
) -> Result<String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (queue_dir, _) = project_config.resolve_linkedin_paths(project_path, lang);
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
    let path = free_path(&queue_dir, &slugify(topic), "md");
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

pub fn publish_linkedin_post(project_path: &str, project_config: &ProjectConfig, lang: &str, filename: &str) -> Result<()> {
    let (queue, published) = project_config.resolve_linkedin_paths(project_path, lang);
    let src = queue.join(filename);
    std::fs::create_dir_all(&published)?;
    let dst = published.join(filename);

    let content = std::fs::read_to_string(&src)?;
    let updated = frontmatter::set(&content, "status", &Field::Text("published".into()), Format::Yaml);
    let updated = frontmatter::set(&updated, "date", &Field::Text(today_like("")), Format::Yaml);

    std::fs::write(&dst, updated)?;
    std::fs::remove_file(&src)?;
    Ok(())
}

pub fn delete_linkedin_queued(project_path: &str, project_config: &ProjectConfig, lang: &str, filename: &str) -> Result<()> {
    let (queue, _) = project_config.resolve_linkedin_paths(project_path, lang);
    std::fs::remove_file(queue.join(filename))?;
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
        .json(&body).send().await.with_context(|| format!("{name} request failed"))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let e = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("{name} error {s}: {e}"));
    }

    let json: serde_json::Value = resp.json().await.with_context(|| format!("{name} parse failed"))?;
    json["choices"][0]["message"]["content"]
        .as_str().map(String::from)
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
    let json_str = raw.trim()
        .strip_prefix("```json").or(raw.trim().strip_prefix("```"))
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
    let body = if project_config.title_heading { format!("# {title}\n\n{body}") } else { body.to_string() };
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
    let extension = if extension.is_empty() { "md" } else { extension };
    let folder = (1..)
        .map(|n| if n == 1 { dir.join(slug) } else { dir.join(format!("{slug}-{n}")) })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(slug));
    folder.join(format!("index.{extension}"))
}

/// How many posts (languages) a bundle folder holds.
fn bundle_posts(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .map(|d| d.filter_map(|e| e.ok()).filter(|e| crate::state::is_bundle_index(&e.file_name().to_string_lossy())).count())
        .unwrap_or(0)
}

/// A post's own folder, for one that is a page bundle (`my-post/index.md`).
fn bundle_folder(posts_dir: &Path, filename: &str) -> Option<PathBuf> {
    filename.split_once('/').map(|(folder, _)| posts_dir.join(folder))
}

/// `dir/slug.ext`, or `dir/slug-2.ext` and so on when that is taken: a new post never
/// replaces one that is there.
fn free_path(dir: &Path, slug: &str, extension: &str) -> PathBuf {
    let slug = if slug.is_empty() { "post" } else { slug };
    let extension = if extension.is_empty() { "md" } else { extension };
    (1..)
        .map(|n| if n == 1 { dir.join(format!("{slug}.{extension}")) } else { dir.join(format!("{slug}-{n}.{extension}")) })
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
        let mut name = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        // A bundle is named by its folder: every one of them holds an index.md.
        if crate::state::is_bundle_index(&name) {
            if let Some(folder) = self.path.parent().and_then(|p| p.file_name()) {
                name = format!("{}/{name}", folder.to_string_lossy());
            }
        }
        let mut line = match &self.command {
            None => format!("Published {name}."),
            Some(run) if run.ok => format!("Published {name}; {} ran.", run.command),
            Some(run) => format!("Published {name}, but {} failed: {}", run.command, run.output),
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
        [&self.command, &self.deploy].iter().all(|r| r.as_ref().is_none_or(|r| r.ok))
    }
}

/// Publishes a draft. With a drafts folder it moves into the blog's; with `draft: true` it
/// stays where it is and loses the mark. Either way it is dated today, then the site's
/// publish command runs. The command failing doesn't undo the publishing: it is reported.
pub fn publish_post(project_path: &str, lang: &str, filename: &str, project_config: &ProjectConfig) -> Result<Published> {
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
    updated = frontmatter::set(&updated, "date", &Field::Text(today_like(&date)), format);

    // A bundle moves as a whole, its images with it.
    if let Some(folder) = bundle_folder(&queue_path, filename) {
        std::fs::write(&src, &updated)?;
        let dst_folder = bundle_folder(&blog_path, filename).unwrap_or_else(|| folder.clone());
        if dst_folder != folder {
            if dst_folder.exists() {
                bail!("{} is already in the blog", dst_folder.display());
            }
            std::fs::create_dir_all(&blog_path)?;
            std::fs::rename(&folder, &dst_folder).with_context(|| format!("{}", folder.display()))?;
        }
        crate::plan::forget(project_path, lang, filename);
        return Ok(after_publish(project_path, project_config, blog_path.join(filename)));
    }

    let mut name = filename.to_string();
    if project_config.date_in_filename && !name.get(..11).is_some_and(crate::state::is_date_prefix) {
        name = format!("{}-{name}", chrono::Local::now().format("%Y-%m-%d"));
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
    Published { path, command, deploy }
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
        let post = crate::state::load_posts(project_path).blog(&lang).queue.into_iter().find(|p| p.filename == filename);
        if let Some(post) = post {
            let issues = crate::checks::check(&post, project_path, &project_config);
            let problems = crate::checks::problems(&issues);
            if !problems.is_empty() {
                let _ = crate::plan::update(project_path, &lang, &filename, |e| e.publish_at = None);
                let why: Vec<&str> = problems.iter().map(|i| i.message.as_str()).collect();
                done.push((
                    format!("{lang}/{filename}"),
                    Err(anyhow::anyhow!("held back and unscheduled: {}", why.join(" "))),
                ));
                continue;
            }
        }
        let result = publish_post(project_path, &lang, &filename, &project_config);
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
            CommandRun { command: command.to_string(), ok: out.status.success(), output: shorten(text.trim()) }
        }
        Err(e) => CommandRun { command: command.to_string(), ok: false, output: e.to_string() },
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
        let sh = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/sh".into());
        let mut c = std::process::Command::new(sh);
        c.args(["-lc", command]);
        c
    }
}

/// Copies an image into the site's images folder, under a name of its own, and gives the
/// address to link it at. A page bundle keeps its images in its own folder, linked by name.
pub fn add_image(project_path: &str, project_config: &ProjectConfig, source: &Path, post: &Path) -> Result<String> {
    let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !["png", "jpg", "jpeg", "gif", "webp", "svg", "avif"].contains(&extension.as_str()) {
        anyhow::bail!("{} is not an image PublishKit adds (png, jpg, gif, webp, svg, avif)", source.display());
    }
    let stem = source.file_stem().map(|s| slugify(&s.to_string_lossy())).unwrap_or_default();
    let stem = if stem.is_empty() { "image".to_string() } else { stem };
    if post.file_name().and_then(|n| n.to_str()).is_some_and(crate::state::is_bundle_index) {
        let folder = post.parent().context("a bundle's post is in a folder")?;
        let target = free_path(folder, &stem, &extension);
        std::fs::copy(source, &target).with_context(|| format!("{}", source.display()))?;
        return Ok(target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    }
    let (dir, url) = project_config.images(project_path);
    let dir = Path::new(project_path).join(dir);
    std::fs::create_dir_all(&dir)?;
    let target = free_path(&dir, &stem, &extension);
    std::fs::copy(source, &target).with_context(|| format!("{}", source.display()))?;
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(format!("{}/{name}", url.trim_end_matches('/')))
}

/// The front matter key a post keeps its description under.
pub fn description_key(content: &str) -> &'static str {
    let fields = frontmatter::fields(content);
    ["description", "summary", "excerpt"].into_iter().find(|k| fields.contains_key(*k)).unwrap_or("description")
}

/// A draft of `content` in another language, beside the posts of that language under the same
/// file name, so the two are paired. Refused when that language already has the post.
pub async fn translate_post(
    project_path: &str,
    project_config: &ProjectConfig,
    content: &str,
    filename: &str,
    from: &str,
    to: &str,
    provider: &LlmProvider,
    api_key: &str,
) -> Result<PathBuf> {
    let (queue, blog) = project_config.resolve_paths(project_path, to);
    let filename = &project_config.counterpart(filename, from, to);
    let target = queue.join(filename);
    if target.exists() || blog.join(filename).exists() {
        anyhow::bail!("{} already has {filename}", language_name(to));
    }

    let fields = frontmatter::fields(content);
    let key = description_key(content);
    let title = frontmatter::text(&fields, "title").unwrap_or_default();
    let description = frontmatter::text(&fields, key).unwrap_or_default();
    let (from_name, to_name) = (language_name(from), language_name(to));

    let system = format!(
        "You translate blog posts from {from_name} into {to_name}, as a native {to_name} writer would \
        put them: natural, not word for word, with the same tone, structure and markdown. Keep code, \
        commands, URLs, image paths and product names as they are. {}",
        project_config.system_prompt
    );
    let body_prompt = format!(
        "Translate this markdown into {to_name}. Output ONLY the translated markdown.\n\n{}",
        frontmatter::body(content)
    );
    let meta_prompt = format!(
        "Translate this blog post's title and description into {to_name}. Reply with ONLY a JSON object \
        (no markdown fences): {{\"title\": \"...\", \"description\": \"...\"}}\n\n\
        Title: {title}\nDescription: {description}"
    );
    let ask = |user: String| {
        let system = system.clone();
        async move { call(provider, &system, &user, api_key).await }
    };
    let (body, meta) = tokio::join!(ask(body_prompt), ask(meta_prompt));
    let body = body?;
    let meta = meta?;
    let meta = meta.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```");
    let meta: serde_json::Value = serde_json::from_str(meta.trim()).context("the model's title and description weren't JSON")?;

    let format = frontmatter::split(content).map_or(project_config.front_matter, |p| p.format);
    let text = |s: &str| Field::Text(s.to_string());
    let mut out = content.to_string();
    if let Some(t) = meta["title"].as_str() {
        out = frontmatter::set(&out, "title", &text(t), format);
    }
    if let Some(d) = meta["description"].as_str().filter(|_| fields.contains_key(key)) {
        out = frontmatter::set(&out, key, &text(d), format);
    }
    for lang_key in ["lang", "locale", "language"] {
        if fields.contains_key(lang_key) {
            out = frontmatter::set(&out, lang_key, &text(to), format);
        }
    }
    out = frontmatter::set(&out, "date", &text(&today_like(&frontmatter::text(&fields, "date").unwrap_or_default())), format);
    match project_config.drafts {
        Drafts::Folder => out = frontmatter::set(&out, "status", &text("draft"), format),
        Drafts::Flag => out = frontmatter::set(&out, "draft", &Field::Bool(true), format),
    }
    let out = frontmatter::with_body(&out, &body);

    std::fs::create_dir_all(target.parent().unwrap_or(&queue))?;
    std::fs::write(&target, out)?;
    Ok(target)
}

/// `node`, or on Windows the per-user copy that setup-windows.ps1 unpacks, when this process
/// started before that copy was added to PATH (e.g. launched from the installer's last page).
#[cfg(windows)]
fn node_command() -> std::path::PathBuf {
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let user_node = std::path::Path::new(&local).join(r"Programs\nodejs\node.exe");
        let on_path = std::process::Command::new("node").arg("--version")
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().is_ok();
        if !on_path && user_node.exists() {
            return user_node;
        }
    }
    "node".into()
}

/// Deletes a draft and its teasers. A published post is refused: with `draft: true` drafts,
/// both sit in the same folder.
pub fn delete_queued(project_path: &str, lang: &str, filename: &str, project_config: &ProjectConfig) -> Result<()> {
    let (queue_path, _) = project_config.resolve_paths(project_path, lang);
    let path = queue_path.join(filename);
    if project_config.drafts == Drafts::Flag {
        let content = std::fs::read_to_string(&path)?;
        if frontmatter::flag(&frontmatter::fields(&content), "draft") != Some(true) {
            anyhow::bail!("{filename} is published, not a draft");
        }
    }
    match bundle_folder(&queue_path, filename) {
        // The bundle's folder holds this post and what belongs to it — unless it holds the
        // post in other languages too, which stay with the images.
        Some(folder) if bundle_posts(&folder) <= 1 => std::fs::remove_dir_all(&folder)?,
        Some(_) => std::fs::remove_file(&path)?,
        None => {
            std::fs::remove_file(&path)?;
            let _ = std::fs::remove_file(social_path(&path)); // ignore if absent
        }
    }
    crate::plan::forget(project_path, lang, filename);
    Ok(())
}

fn slugify(title: &str) -> String {
    title.to_lowercase()
        .chars()
        .map(|c| match c {
            'à'|'â'|'ä' => 'a', 'é'|'è'|'ê'|'ë' => 'e',
            'î'|'ï' => 'i', 'ô'|'ö' => 'o', 'ù'|'û'|'ü' => 'u',
            'ç' => 'c', 'ñ' => 'n', 'á'|'ã'|'å' => 'a', 'í'|'ì' => 'i', 'ó'|'ò'|'õ'|'ø' => 'o',
            'ú' => 'u', 'ý'|'ÿ' => 'y', ' '|'_'|'/'|'.' => '-', c => c,
        })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{load_posts, Preset};

    fn site(files: &[(&str, &str)]) -> (PathBuf, String) {
        // A folder of its own per test: tests run in parallel, and a clock can repeat.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("publishkit-api-{}-{n}", std::process::id()));
        for (path, content) in files {
            let p = root.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        let project = root.to_string_lossy().into_owned();
        (root, project)
    }

    #[test]
    fn a_flagged_draft_is_published_in_place_and_keeps_its_other_fields() {
        let (root, project) = site(&[
            ("hugo.toml", ""),
            ("content/posts/a.md", "+++\ntitle = \"A\"\n# mine\ndate = 2026-01-01T09:00:00Z\ndraft = true\n[params]\nx = 1\n+++\nBody"),
        ]);
        let cfg = ProjectConfig::load(&project);
        assert_eq!(cfg.preset, Some(Preset::Hugo));
        let done = publish_post(&project, "en", "a.md", &cfg).unwrap();
        assert_eq!(done.path, root.join("content/posts/a.md"));
        assert_eq!(done.command, None);
        let content = std::fs::read_to_string(&done.path).unwrap();
        assert!(content.contains("draft = false\n[params]\nx = 1\n+++\nBody"), "{content}");
        assert!(content.contains("# mine") && !content.contains("status"), "{content}");
        assert!(content.contains(&format!("date = \"{}T", chrono::Local::now().format("%Y-%m-%d"))), "{content}");
        assert!(delete_queued(&project, "en", "a.md", &cfg).is_err(), "a published post is not a draft");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_jekyll_draft_moves_into_posts_with_the_date_in_its_name() {
        let (root, project) = site(&[("_config.yml", ""), ("_posts/2020-01-01-old.md", "---\ntitle: Old\n---\n")]);
        let cfg = ProjectConfig::load(&project);
        let path = save_to_queue(&project, "Hello, World", "Sum", "en", "Body", &cfg).unwrap();
        assert!(path.ends_with("_drafts/hello-world.md"), "{path}");
        let draft = std::fs::read_to_string(&path).unwrap();
        assert!(!draft.contains("# Hello") && draft.contains("status: \"draft\""), "{draft}");
        assert_eq!(load_posts(&project).blog("en").queue.len(), 1);

        let done = publish_post(&project, "en", "hello-world.md", &cfg).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d");
        assert_eq!(done.path, root.join(format!("_posts/{today}-hello-world.md")));
        assert!(!Path::new(&path).exists());
        let shelf = load_posts(&project).blog("en");
        assert_eq!((shelf.queue.len(), shelf.published.len()), (0, 2));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_failing_publish_command_is_reported_and_the_post_stays_published() {
        let (root, project) = site(&[("content/fr/queue/a.md", "---\ntitle: \"A\"\nstatus: \"draft\"\n---\n\nBody")]);
        let mut cfg = ProjectConfig::load(&project);
        assert_eq!(cfg.preset, Some(Preset::PublishKit));
        cfg.publish_command = "echo built && echo broken >&2 && exit 3".into();
        let done = publish_post(&project, "fr", "a.md", &cfg).unwrap();
        let run = done.command.clone().unwrap();
        assert!(!run.ok);
        assert_eq!(run.output, "broken");
        assert!(done.summary().contains("failed: broken"));
        assert!(!done.ok());
        assert_eq!(done.deploy, None);
        let published = std::fs::read_to_string(root.join("content/fr/blog/a.md")).unwrap();
        assert!(published.contains("status: \"published\""), "{published}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_image_is_copied_where_the_site_serves_it_under_a_name_of_its_own() {
        let (root, project) = site(&[("hugo.toml", ""), ("content/posts/a.md", "+++\n+++\n"), ("in/My Photo.PNG", "png")]);
        let cfg = ProjectConfig::load(&project);
        let source = root.join("in/My Photo.PNG");
        let post = root.join("content/posts/a.md");
        assert_eq!(add_image(&project, &cfg, &source, &post).unwrap(), "/images/my-photo.png");
        assert_eq!(add_image(&project, &cfg, &source, &post).unwrap(), "/images/my-photo-2.png");
        assert!(root.join("static/images/my-photo-2.png").is_file());
        assert!(add_image(&project, &cfg, &root.join("hugo.toml"), &post).is_err());
        let bundle = root.join("content/posts/b/index.md");
        std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
        assert_eq!(add_image(&project, &cfg, &source, &bundle).unwrap(), "my-photo.png");
        assert!(root.join("content/posts/b/my-photo.png").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_scheduled_draft_is_published_once_its_time_has_come() {
        let (root, project) = site(&[
            ("content/en/queue/due.md", "---\ntitle: Due\nstatus: draft\n---\nText."),
            ("content/en/queue/later.md", "---\ntitle: Later\nstatus: draft\n---\nText."),
            ("content/en/queue/unfinished.md", "---\ntitle: Unfinished\nstatus: draft\n---\nTODO"),
        ]);
        let at = |when: &'static str| move |e: &mut crate::plan::Entry| e.publish_at = Some(when.to_string());
        crate::plan::update(&project, "en", "due.md", at("2020-01-01T09:00")).unwrap();
        crate::plan::update(&project, "en", "later.md", at("2999-01-01T09:00")).unwrap();
        crate::plan::update(&project, "en", "gone.md", at("2020-01-01T09:00")).unwrap();
        crate::plan::update(&project, "en", "unfinished.md", at("2020-01-01T08:00")).unwrap();
        assert_eq!(load_posts(&project).blog("en").queue.iter().filter(|p| p.publish_at.is_some()).count(), 3);

        let done = publish_due(&project);
        assert_eq!(done.len(), 2);
        assert_eq!(done[0].0, "en/unfinished.md");
        assert!(done[0].1.as_ref().unwrap_err().to_string().contains("TODO"));
        assert!(root.join("content/en/queue/unfinished.md").is_file());
        assert_eq!(done[1].0, "en/due.md");
        assert!(root.join("content/en/blog/due.md").is_file());
        let plan = crate::plan::Plan::load(&project);
        assert_eq!(plan.drafts.keys().collect::<Vec<_>>(), ["en/later.md"]);
        assert!(publish_due(&project).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_site_that_deploys_after_publishing_does_so_once_its_index_is_built() {
        let (root, project) = site(&[("content/en/queue/a.md", "---\ntitle: A\nstatus: draft\n---\nText.")]);
        let mut cfg = ProjectConfig::load(&project);
        cfg.publish_command = "true".into();
        cfg.deploy_command = "echo deployed > deployed.txt".into();
        cfg.deploy_after_publish = true;
        let done = publish_post(&project, "en", "a.md", &cfg).unwrap();
        assert!(done.ok() && done.deploy.is_some(), "{done:?}");
        assert!(root.join("deployed.txt").is_file());
        assert!(done.summary().ends_with("Deployed."));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_bundle_is_written_published_and_deleted_as_a_folder() {
        let (root, project) = site(&[
            ("content/en/queue/old/index.md", "---\ntitle: Old\nstatus: draft\n---\nText."),
            ("content/en/queue/old/cover.png", "png"),
        ]);
        let mut cfg = ProjectConfig::load(&project);
        let done = publish_post(&project, "en", "old/index.md", &cfg).unwrap();
        assert_eq!(done.path, root.join("content/en/blog/old/index.md"));
        assert_eq!(done.summary(), "Published old/index.md.");
        assert!(root.join("content/en/blog/old/cover.png").is_file());
        assert!(!root.join("content/en/queue/old").exists());
        assert!(std::fs::read_to_string(&done.path).unwrap().contains("status: \"published\""));

        cfg.bundles = true;
        let path = save_to_queue(&project, "New one", "s", "en", "Body", &cfg).unwrap();
        assert!(path.ends_with("content/en/queue/new-one/index.md"), "{path}");
        assert_eq!(load_posts(&project).blog("en").queue[0].filename, "new-one/index.md");
        delete_queued(&project, "en", "new-one/index.md", &cfg).unwrap();
        assert!(!root.join("content/en/queue/new-one").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_new_post_never_replaces_one_with_the_same_title() {
        let (root, project) = site(&[("content/en/queue/.keep", "")]);
        let cfg = ProjectConfig::load(&project);
        let first = save_to_queue(&project, "Same", "s", "en", "1", &cfg).unwrap();
        let second = save_to_queue(&project, "Same", "s", "en", "2", &cfg).unwrap();
        assert!(first.ends_with("same.md") && second.ends_with("same-2.md"), "{first} {second}");
        assert!(std::fs::read_to_string(&first).unwrap().contains("# Same\n\n1"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

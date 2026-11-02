//! Publishing: moving a draft live, running the site's deploy command, and posts that are due.

use super::*;

/// Moves a network's draft to the posted ones, dated `at` when it went out earlier (posted
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
pub(super) fn after_publish(
    project_path: &str,
    project_config: &ProjectConfig,
    path: PathBuf,
) -> Published {
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

pub(super) fn run_command(project_path: &str, command: &str) -> Option<CommandRun> {
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
pub(super) fn shorten(text: &str) -> String {
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
pub(super) fn shell(command: &str) -> std::process::Command {
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

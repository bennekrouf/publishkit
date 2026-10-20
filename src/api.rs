use anyhow::{Context, Result};
use crate::state::{LlmProvider, ProjectConfig, SocialPosts};

pub async fn generate_linkedin_post(
    topic: &str,
    angle: &str,
    lang: &str,
    provider: &LlmProvider,
    api_key: &str,
) -> Result<String> {
    let lang_instruction = if lang == "fr" { "Write entirely in French." } else { "Write entirely in English." };

    let system = format!(
        "You write LinkedIn posts for a company page promoting Mayorana's products. \
        {lang_instruction} Professional but engaging tone, concrete value proposition, \
        no corporate fluff. Max 3 hashtags. 4-8 short paragraphs/lines, easy to skim."
    );
    let user = format!(
        "Write a LinkedIn post about: \"{topic}\"\n\
        Angle / key points: {angle}\n\n\
        Output ONLY the post text, ready to publish (no markdown headers, no frontmatter)."
    );

    match provider {
        LlmProvider::DeepSeek => call_deepseek(&system, &user, api_key).await,
        LlmProvider::Claude   => call_claude(&system, &user, api_key).await,
    }
}

pub fn save_linkedin_to_queue(
    project_path: &str,
    topic: &str,
    lang: &str,
    body: &str,
) -> Result<String> {
    let slug = slugify(topic);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let queue_dir = std::path::PathBuf::from(project_path)
        .join("content").join(lang).join("linkedin").join("queue");
    std::fs::create_dir_all(&queue_dir)?;

    let escaped_topic = topic.replace('"', "\\\"");
    let content = format!(
        "---\ntopic: \"{escaped_topic}\"\ndate: \"{today}\"\nlang: \"{lang}\"\nstatus: \"draft\"\n---\n\n{body}"
    );
    let path = queue_dir.join(format!("{slug}.md"));
    std::fs::write(&path, content)?;
    Ok(path.to_string_lossy().to_string())
}

pub fn publish_linkedin_post(project_path: &str, lang: &str, filename: &str) -> Result<()> {
    let root = std::path::PathBuf::from(project_path).join("content").join(lang).join("linkedin");
    let src = root.join("queue").join(filename);
    let dst_dir = root.join("published");
    std::fs::create_dir_all(&dst_dir)?;
    let dst = dst_dir.join(filename);

    let content = std::fs::read_to_string(&src)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let updated = content.replace("status: \"draft\"", "status: \"published\"");
    let updated = if let Some(line) = updated.lines().find(|l| l.starts_with("date:")) {
        updated.replace(line, &format!("date: \"{today}\""))
    } else { updated };

    std::fs::write(&dst, updated)?;
    std::fs::remove_file(&src)?;
    Ok(())
}

pub fn delete_linkedin_queued(project_path: &str, lang: &str, filename: &str) -> Result<()> {
    let path = std::path::PathBuf::from(project_path)
        .join("content").join(lang).join("linkedin").join("queue").join(filename);
    std::fs::remove_file(&path)?;
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
    let lang_instruction = if lang == "fr" { "Write entirely in French." } else { "Write entirely in English." };

    let system = format!("{} {lang_instruction}", project_config.system_prompt);
    let user = format!(
        "Write a blog post with this title: \"{title}\"\n\
        Summary / angle: {summary}\n\n\
        Output ONLY the markdown body (no frontmatter). Start directly with the first paragraph. \
        Structure: intro, 3-4 sections with ## headings, a conclusion. Aim for ~600-800 words."
    );

    match provider {
        LlmProvider::DeepSeek => call_deepseek(&system, &user, api_key).await,
        LlmProvider::Claude   => call_claude(&system, &user, api_key).await,
    }
}

async fn call_deepseek(system: &str, user: &str, api_key: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": LlmProvider::DeepSeek.model(),
        "messages": [
            {"role": "system", "content": system},
            {"role": "user",   "content": user}
        ],
        "temperature": 0.75
    });

    let resp = client
        .post(LlmProvider::DeepSeek.api_url())
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send().await.context("DeepSeek request failed")?;

    if !resp.status().is_success() {
        let s = resp.status();
        let e = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("DeepSeek error {s}: {e}"));
    }

    let json: serde_json::Value = resp.json().await.context("DeepSeek parse failed")?;
    json["choices"][0]["message"]["content"]
        .as_str().map(String::from)
        .context("Empty DeepSeek response")
}

async fn call_claude(system: &str, user: &str, api_key: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": LlmProvider::Claude.model(),
        "max_tokens": 2048,
        "system": system,
        "messages": [{"role": "user", "content": user}]
    });

    let resp = client
        .post(LlmProvider::Claude.api_url())
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send().await.context("Claude request failed")?;

    if !resp.status().is_success() {
        let s = resp.status();
        let e = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Claude error {s}: {e}"));
    }

    let json: serde_json::Value = resp.json().await.context("Claude parse failed")?;
    json["content"][0]["text"]
        .as_str().map(String::from)
        .context("Empty Claude response")
}

pub async fn generate_social_posts(
    title: &str,
    summary: &str,
    lang: &str,
    blog_url: &str,
    provider: &LlmProvider,
    api_key: &str,
) -> Result<SocialPosts> {
    let lang_instruction = if lang == "fr" { "Write in French." } else { "Write in English." };

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

    let raw = match provider {
        LlmProvider::DeepSeek => call_deepseek(&system, &user, api_key).await?,
        LlmProvider::Claude   => call_claude(&system, &user, api_key).await?,
    };

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
    let slug = slugify(title);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (queue_path, _) = project_config.resolve_paths(project_path, lang);
    let queue_dir = queue_path.to_string_lossy().to_string();
    std::fs::create_dir_all(&queue_dir)?;

    let tags_str = project_config.default_tags.iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let content = format!(
        "---\ntitle: \"{title}\"\ndescription: \"{summary}\"\ndate: \"{today}\"\n\
        author: \"{}\"\ntags: [{tags_str}]\n\
        lang: \"{lang}\"\nstatus: \"draft\"\n---\n\n# {title}\n\n{body}",
        project_config.author
    );
    let path = format!("{}/{}.md", queue_dir, slug);
    std::fs::write(&path, content)?;
    Ok(path)
}

pub fn save_social(md_path: &str, social: &SocialPosts) -> Result<()> {
    let social_path = md_path.replace(".md", ".social.json");
    let json = serde_json::to_string_pretty(social)?;
    std::fs::write(social_path, json)?;
    Ok(())
}

pub fn publish_post(project_path: &str, lang: &str, filename: &str, project_config: &ProjectConfig) -> Result<()> {
    let (queue_path, blog_path) = project_config.resolve_paths(project_path, lang);
    let src = queue_path.join(filename).to_string_lossy().to_string();
    let dst_dir = blog_path.to_string_lossy().to_string();
    std::fs::create_dir_all(&dst_dir)?;
    let dst = format!("{}/{}", dst_dir, filename);

    let content = std::fs::read_to_string(&src)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let updated = content.replace("status: \"draft\"", "status: \"published\"");
    let updated = if let Some(line) = updated.lines().find(|l| l.starts_with("date:")) {
        updated.replace(line, &format!("date: \"{today}\""))
    } else { updated };

    std::fs::write(&dst, updated)?;
    std::fs::remove_file(&src)?;

    // Move social sidecar if it exists
    let social_src = src.replace(".md", ".social.json");
    let social_dst = dst.replace(".md", ".social.json");
    if std::path::Path::new(&social_src).exists() {
        let _ = std::fs::copy(&social_src, &social_dst);
        let _ = std::fs::remove_file(&social_src);
    }

    let _ = std::process::Command::new(node_command())
        .arg("scripts/generate-blog-data.js")
        .current_dir(project_path)
        .status();
    Ok(())
}

/// `node`, or on Windows the per-user copy that setup-windows.ps1 unpacks, when this process
/// started before that copy was added to PATH (e.g. launched from the installer's last page).
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

pub fn delete_queued(project_path: &str, lang: &str, filename: &str, project_config: &ProjectConfig) -> Result<()> {
    let (queue_path, _) = project_config.resolve_paths(project_path, lang);
    let path = queue_path.join(filename).to_string_lossy().to_string();
    std::fs::remove_file(&path)?;
    let social = path.replace(".md", ".social.json");
    let _ = std::fs::remove_file(social); // ignore if absent
    Ok(())
}

fn slugify(title: &str) -> String {
    title.to_lowercase()
        .chars()
        .map(|c| match c {
            'à'|'â'|'ä' => 'a', 'é'|'è'|'ê'|'ë' => 'e',
            'î'|'ï' => 'i', 'ô'|'ö' => 'o', 'ù'|'û'|'ü' => 'u',
            'ç' => 'c', ' '|'_' => '-', c => c,
        })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-")
}

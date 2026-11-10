//! Writing posts with the language model: blog posts, social posts, and where their link goes.

use super::*;

pub(crate) fn write_in(lang: &str) -> String {
    format!("Write entirely in {}.", language_name(lang))
}

/// A network's post written by the model, from what it is about and the points to make;
/// `link`, when there is one, ends up as it is, on a line of its own, where it can be clicked.
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
        Channel::Instagram => format!(
            "You write Instagram captions for {company}. {lang_instruction} The caption goes \
            under a photo or a video: open with a line that makes people stop scrolling, then a \
            few short lines, warm and concrete, no corporate fluff. A few emoji at most. End with \
            at most 5 relevant hashtags on a line of their own: Instagram takes no more. At most \
            2,000 characters."
        ),
        Channel::TikTok => format!(
            "You write TikTok captions for {company}. {lang_instruction} The caption goes under a \
            video: short, casual and direct, a hook in the first line, one idea. End with 3 to 5 \
            relevant hashtags. At most 300 characters."
        ),
    };
    let user = social_prompt(channel, topic, angle, link);
    let text = call(provider, &system, &user, api_key).await?;
    if channel.links_clickable() {
        Ok(place_link(text.trim(), link.trim()))
    } else {
        Ok(text.trim().to_string())
    }
}

/// What the model is asked for a network's post.
pub(super) fn social_prompt(channel: Channel, topic: &str, angle: &str, link: &str) -> String {
    let link = link.trim();
    // LinkedIn and X make a bare address clickable, and show no anchor text or markdown.
    // Instagram and TikTok show it as plain text: people are sent to the profile instead.
    let link_rule = if link.is_empty() {
        String::new()
    } else if !channel.links_clickable() {
        format!(
            "The post is about this page: {link}\n\
            Do not put the address in the caption, where it can't be clicked: say the link is \
            in the bio instead.\n"
        )
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
pub(super) fn trim_url_end(url: &str) -> &str {
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
pub(crate) async fn call(
    provider: &LlmProvider,
    system: &str,
    user: &str,
    api_key: &str,
) -> Result<String> {
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
        .map_err(|e| {
            anyhow::anyhow!(crate::api0::network_error(
                &format!("{name} request failed"),
                &e
            ))
        })?;

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

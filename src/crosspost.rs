//! Copies of a published post on other platforms — dev.to, Hashnode, a WordPress or Ghost
//! blog — pointing back at the original as canonical, so search engines credit the site.
//!
//! The accounts are the person's, kept in the app's own config beside the model's key, never
//! in the site's repository. Where each post went is kept in `.publishkit/crossposts.json`, so
//! sending a post again updates its copy instead of making a second one.

use crate::frontmatter;
use anyhow::{bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Accounts {
    pub devto_key: String,
    pub hashnode_token: String,
    pub hashnode_publication: String,
    /// `https://example.com`, the WordPress site.
    pub wordpress_url: String,
    pub wordpress_user: String,
    /// An application password (Users → Profile → Application Passwords), not the login one.
    pub wordpress_password: String,
    /// `https://example.ghost.io`, the Ghost site.
    pub ghost_url: String,
    /// A custom integration's Admin API key: `id:secret`.
    pub ghost_key: String,
    /// Copies are sent as drafts, to be published there after a look.
    pub as_draft: bool,
    /// `name.bsky.social`, the Bluesky account the teasers are posted from.
    pub bluesky_handle: String,
    /// An app password (Settings → Privacy and security → App passwords), not the account's.
    pub bluesky_password: String,
    /// The account's server; empty: `https://bsky.social`.
    pub bluesky_service: String,
    /// `mastodon.social`, the account's server.
    pub mastodon_instance: String,
    /// An access token with write:statuses (Preferences → Development → New application).
    pub mastodon_token: String,
}

impl Default for Accounts {
    fn default() -> Self {
        Self {
            devto_key: String::new(),
            hashnode_token: String::new(),
            hashnode_publication: String::new(),
            wordpress_url: String::new(),
            wordpress_user: String::new(),
            wordpress_password: String::new(),
            ghost_url: String::new(),
            ghost_key: String::new(),
            as_draft: true,
            bluesky_handle: String::new(),
            bluesky_password: String::new(),
            bluesky_service: String::new(),
            mastodon_instance: String::new(),
            mastodon_token: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    DevTo,
    Hashnode,
    WordPress,
    Ghost,
}

impl Target {
    pub const ALL: [Target; 4] = [
        Target::DevTo,
        Target::Hashnode,
        Target::WordPress,
        Target::Ghost,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Target::DevTo => "dev.to",
            Target::Hashnode => "Hashnode",
            Target::WordPress => "WordPress",
            Target::Ghost => "Ghost",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Target::DevTo => "devto",
            Target::Hashnode => "hashnode",
            Target::WordPress => "wordpress",
            Target::Ghost => "ghost",
        }
    }

    pub fn from_key(key: &str) -> Option<Target> {
        Target::ALL.into_iter().find(|t| t.key() == key)
    }

    pub fn configured(self, accounts: &Accounts) -> bool {
        let set = |s: &str| !s.trim().is_empty();
        match self {
            Target::DevTo => set(&accounts.devto_key),
            Target::Hashnode => {
                set(&accounts.hashnode_token) && set(&accounts.hashnode_publication)
            }
            Target::WordPress => {
                set(&accounts.wordpress_url)
                    && set(&accounts.wordpress_user)
                    && set(&accounts.wordpress_password)
            }
            Target::Ghost => set(&accounts.ghost_url) && accounts.ghost_key.contains(':'),
        }
    }
}

/// Where one post's copy is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Copy {
    pub id: String,
    pub url: String,
}

/// `lang/filename` → platform → copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Record {
    #[serde(default)]
    pub posts: BTreeMap<String, BTreeMap<Target, Copy>>,
}

fn record_path(project_path: &str) -> PathBuf {
    Path::new(project_path)
        .join(".publishkit")
        .join("crossposts.json")
}

impl Record {
    pub fn load(project_path: &str) -> Record {
        std::fs::read_to_string(record_path(project_path))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn copies(&self, lang: &str, filename: &str) -> BTreeMap<Target, Copy> {
        self.posts
            .get(&format!("{lang}/{filename}"))
            .cloned()
            .unwrap_or_default()
    }

    fn save_copy(
        project_path: &str,
        lang: &str,
        filename: &str,
        target: Target,
        copy: Copy,
    ) -> Result<()> {
        let mut record = Record::load(project_path);
        record
            .posts
            .entry(format!("{lang}/{filename}"))
            .or_default()
            .insert(target, copy);
        let path = record_path(project_path);
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(project_path)))?;
        std::fs::write(path, serde_json::to_string_pretty(&record)?)?;
        Ok(())
    }
}

/// The post as the platforms get it.
#[derive(Clone, Debug, PartialEq)]
pub struct Article {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    /// Links and images point at the site, whole addresses.
    pub markdown: String,
    pub canonical_url: String,
}

impl Article {
    pub fn of(content: &str, site_url: &str, canonical_url: &str) -> Article {
        let fields = frontmatter::fields(content);
        let title = frontmatter::text(&fields, "title").unwrap_or_default();
        let description =
            frontmatter::text(&fields, crate::api::description_key(content)).unwrap_or_default();
        let mut body = frontmatter::body(content).to_string();
        // The platforms print the title themselves.
        if let Some(rest) = body.strip_prefix(&format!("# {title}")) {
            body = rest.trim_start_matches(['\n', '\r']).to_string();
        }
        let mut markdown = absolute_links(&body, site_url);
        // A bundle's images sit beside it, linked by name: on the site, under its address.
        let base = canonical_url.trim_end_matches('/');
        for (_, _, target) in crate::checks::links(&markdown.clone()) {
            let relative =
                !target.is_empty() && !target.contains(':') && !target.starts_with(['/', '#']);
            if relative && !base.is_empty() {
                markdown = markdown.replace(&format!("]({target}"), &format!("]({base}/{target}"));
            }
        }
        Article {
            title,
            description,
            tags: frontmatter::list(&fields, "tags"),
            markdown,
            canonical_url: canonical_url.to_string(),
        }
    }

    fn html(&self) -> String {
        use pulldown_cmark::{html, Options, Parser};
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        opts.insert(Options::ENABLE_STRIKETHROUGH);
        let mut out = String::new();
        html::push_html(&mut out, Parser::new_ext(&self.markdown, opts));
        out
    }
}

/// `](/images/a.png)` → `](https://site/images/a.png)`: a site-relative link means nothing on
/// another platform.
pub fn absolute_links(markdown: &str, site_url: &str) -> String {
    let site = site_url.trim_end_matches('/');
    if site.is_empty() {
        return markdown.to_string();
    }
    markdown
        .replace("](/", &format!("]({site}/"))
        .replace("src=\"/", &format!("src=\"{site}/"))
        .replace("href=\"/", &format!("href=\"{site}/"))
}

/// dev.to takes four tags, of letters and digits.
fn devto_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = tags
        .iter()
        .map(|t| {
            t.to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .collect();
    out.dedup();
    out.truncate(4);
    out
}

fn slug(tag: &str) -> String {
    tag.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// One HTTP request to a platform, built without sending it.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

pub fn devto_request(accounts: &Accounts, article: &Article, existing: Option<&Copy>) -> Request {
    let body = json!({ "article": {
        "title": article.title,
        "body_markdown": article.markdown,
        "description": article.description,
        "tags": devto_tags(&article.tags),
        "canonical_url": article.canonical_url,
        "published": !accounts.as_draft,
    }});
    let (method, url) = match existing {
        Some(c) => ("PUT", format!("https://dev.to/api/articles/{}", c.id)),
        None => ("POST", "https://dev.to/api/articles".to_string()),
    };
    Request {
        method,
        url,
        headers: vec![
            ("api-key".into(), accounts.devto_key.trim().into()),
            ("accept".into(), "application/vnd.forem.api-v1+json".into()),
        ],
        body,
    }
}

pub fn hashnode_request(
    accounts: &Accounts,
    article: &Article,
    existing: Option<&Copy>,
) -> Request {
    let tags: Vec<Value> = article
        .tags
        .iter()
        .map(|t| json!({ "slug": slug(t), "name": t }))
        .collect();
    let mut input = json!({
        "title": article.title,
        "contentMarkdown": article.markdown,
        "subtitle": article.description,
        "tags": tags,
        "originalArticleURL": article.canonical_url,
    });
    let query = match existing {
        // A copy published there, or still a draft there, is changed in place.
        Some(c) if c.url.contains("/draft/") => {
            input["id"] = json!(c.id);
            "mutation($input: UpdateDraftInput!) { updateDraft(input: $input) { draft { id } } }"
        }
        Some(c) => {
            input["id"] = json!(c.id);
            "mutation($input: UpdatePostInput!) { updatePost(input: $input) { post { id url } } }"
        }
        None => {
            input["publicationId"] = json!(accounts.hashnode_publication.trim());
            if accounts.as_draft {
                "mutation($input: CreateDraftInput!) { createDraft(input: $input) { draft { id } } }"
            } else {
                "mutation($input: PublishPostInput!) { publishPost(input: $input) { post { id url } } }"
            }
        }
    };
    Request {
        method: "POST",
        url: "https://gql.hashnode.com".into(),
        headers: vec![(
            "Authorization".into(),
            accounts.hashnode_token.trim().into(),
        )],
        body: json!({ "query": query, "variables": { "input": input } }),
    }
}

pub fn wordpress_request(
    accounts: &Accounts,
    article: &Article,
    existing: Option<&Copy>,
) -> Request {
    let site = accounts.wordpress_url.trim().trim_end_matches('/');
    let credentials = base64::engine::general_purpose::STANDARD.encode(format!(
        "{}:{}",
        accounts.wordpress_user.trim(),
        accounts.wordpress_password.trim().replace(' ', "")
    ));
    // WordPress has no canonical field of its own: the copy says where it first appeared.
    let content = format!(
        "{}\n<p><em>First published at <a href=\"{url}\">{url}</a>.</em></p>",
        article.html(),
        url = article.canonical_url
    );
    let url = match existing {
        Some(c) => format!("{site}/wp-json/wp/v2/posts/{}", c.id),
        None => format!("{site}/wp-json/wp/v2/posts"),
    };
    Request {
        method: "POST",
        url,
        headers: vec![("Authorization".into(), format!("Basic {credentials}"))],
        body: json!({
            "title": article.title,
            "content": content,
            "excerpt": article.description,
            "status": if accounts.as_draft { "draft" } else { "publish" },
        }),
    }
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.iter().map(|b| b ^ byte).collect::<Vec<u8>>();
    let inner = Sha256::new()
        .chain_update(pad(0x36))
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(pad(0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

/// The short-lived token Ghost's Admin API takes, signed with the integration's secret.
pub fn ghost_token(key: &str, now: i64) -> Result<String> {
    let (id, secret) = key
        .trim()
        .split_once(':')
        .context("a Ghost Admin API key is id:secret")?;
    let secret =
        hex::decode(secret).context("the secret part of a Ghost Admin API key is hexadecimal")?;
    let b64 = |v: &Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
    let header = b64(&json!({ "alg": "HS256", "typ": "JWT", "kid": id }));
    let payload = b64(&json!({ "iat": now, "exp": now + 300, "aud": "/admin/" }));
    let signature = hmac_sha256(&secret, format!("{header}.{payload}").as_bytes());
    Ok(format!(
        "{header}.{payload}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature)
    ))
}

pub fn ghost_request(
    accounts: &Accounts,
    article: &Article,
    existing: Option<(&Copy, &str)>,
    now: i64,
) -> Result<Request> {
    let site = accounts.ghost_url.trim().trim_end_matches('/');
    let mut post = json!({
        "title": article.title,
        "html": article.html(),
        "custom_excerpt": article.description,
        "canonical_url": article.canonical_url,
        "tags": article.tags,
        "status": if accounts.as_draft { "draft" } else { "published" },
    });
    let (method, url) = match existing {
        Some((copy, updated_at)) => {
            post["updated_at"] = json!(updated_at);
            (
                "PUT",
                format!("{site}/ghost/api/admin/posts/{}/?source=html", copy.id),
            )
        }
        None => ("POST", format!("{site}/ghost/api/admin/posts/?source=html")),
    };
    Ok(Request {
        method,
        url,
        headers: vec![(
            "Authorization".into(),
            format!("Ghost {}", ghost_token(&accounts.ghost_key, now)?),
        )],
        body: json!({ "posts": [post] }),
    })
}

async fn send(client: &reqwest::Client, request: &Request) -> Result<Value> {
    let mut builder = match request.method {
        "PUT" => client.put(&request.url),
        "GET" => client.get(&request.url),
        _ => client.post(&request.url),
    };
    for (k, v) in &request.headers {
        builder = builder.header(k, v);
    }
    if request.method != "GET" {
        builder = builder.json(&request.body);
    }
    let response = builder
        .send()
        .await
        .with_context(|| format!("{} couldn't be reached", request.url))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let short: String = text.chars().take(300).collect();
        bail!("{status}: {short}");
    }
    serde_json::from_str(&text).context("the answer wasn't JSON")
}

/// Sends a published post to a platform — or updates the copy sent before — and remembers
/// where it is.
pub async fn crosspost(
    project_path: &str,
    accounts: &Accounts,
    target: Target,
    lang: &str,
    filename: &str,
    article: &Article,
) -> Result<Copy> {
    if !target.configured(accounts) {
        bail!(
            "{} isn't set up: add the account in ⚙ Settings → Cross-posting",
            target.label()
        );
    }
    let existing = Record::load(project_path)
        .copies(lang, filename)
        .get(&target)
        .cloned();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("PublishKit/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let copy = match target {
        Target::DevTo => {
            let answer = send(
                &client,
                &devto_request(accounts, article, existing.as_ref()),
            )
            .await?;
            Copy {
                id: answer["id"]
                    .as_i64()
                    .map(|i| i.to_string())
                    .context("dev.to gave no id")?,
                url: answer["url"]
                    .as_str()
                    .unwrap_or("https://dev.to/dashboard")
                    .to_string(),
            }
        }
        Target::Hashnode => {
            let answer = send(
                &client,
                &hashnode_request(accounts, article, existing.as_ref()),
            )
            .await?;
            if let Some(errors) = answer["errors"].as_array().filter(|e| !e.is_empty()) {
                let messages: Vec<&str> = errors
                    .iter()
                    .filter_map(|e| e["message"].as_str())
                    .collect();
                bail!("Hashnode: {}", messages.join("; "));
            }
            let data = answer["data"]
                .as_object()
                .and_then(|d| d.values().next())
                .cloned()
                .unwrap_or_default();
            if let Some(post) = data.get("post").filter(|p| p.is_object()) {
                Copy {
                    id: post["id"]
                        .as_str()
                        .context("Hashnode gave no id")?
                        .to_string(),
                    url: post["url"].as_str().unwrap_or_default().to_string(),
                }
            } else {
                let id = data["draft"]["id"]
                    .as_str()
                    .context("Hashnode gave no id")?
                    .to_string();
                Copy {
                    url: format!("https://hashnode.com/draft/{id}"),
                    id,
                }
            }
        }
        Target::WordPress => {
            let answer = send(
                &client,
                &wordpress_request(accounts, article, existing.as_ref()),
            )
            .await?;
            Copy {
                id: answer["id"]
                    .as_i64()
                    .map(|i| i.to_string())
                    .context("WordPress gave no id")?,
                url: answer["link"].as_str().unwrap_or_default().to_string(),
            }
        }
        Target::Ghost => {
            let now = chrono::Utc::now().timestamp();
            // Ghost changes a post only when told the version it is changing.
            let updated_at = match &existing {
                Some(copy) => {
                    let site = accounts.ghost_url.trim().trim_end_matches('/');
                    let get = Request {
                        method: "GET",
                        url: format!("{site}/ghost/api/admin/posts/{}/", copy.id),
                        headers: vec![(
                            "Authorization".into(),
                            format!("Ghost {}", ghost_token(&accounts.ghost_key, now)?),
                        )],
                        body: Value::Null,
                    };
                    let answer = send(&client, &get).await?;
                    answer["posts"][0]["updated_at"].as_str().map(String::from)
                }
                None => None,
            };
            let existing = existing.as_ref().zip(updated_at.as_deref());
            let answer = send(&client, &ghost_request(accounts, article, existing, now)?).await?;
            let post = &answer["posts"][0];
            Copy {
                id: post["id"].as_str().context("Ghost gave no id")?.to_string(),
                url: post["url"].as_str().unwrap_or_default().to_string(),
            }
        }
    };
    Record::save_copy(project_path, lang, filename, target, copy.clone())?;
    Ok(copy)
}

#[cfg(test)]
mod tests;

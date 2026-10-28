//! A post's short teaser, posted to Bluesky or Mastodon with the person's own account.

use crate::crosspost::Accounts;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Network {
    Bluesky,
    Mastodon,
}

impl Network {
    pub const ALL: [Network; 2] = [Network::Bluesky, Network::Mastodon];

    pub fn label(self) -> &'static str {
        match self {
            Network::Bluesky => "Bluesky",
            Network::Mastodon => "Mastodon",
        }
    }

    /// The longest post the network takes, in characters.
    pub fn limit(self) -> usize {
        match self {
            Network::Bluesky => 300,
            Network::Mastodon => 500,
        }
    }

    pub fn configured(self, accounts: &Accounts) -> bool {
        let set = |s: &str| !s.trim().is_empty();
        match self {
            Network::Bluesky => set(&accounts.bluesky_handle) && set(&accounts.bluesky_password),
            Network::Mastodon => set(&accounts.mastodon_instance) && set(&accounts.mastodon_token),
        }
    }
}

/// The links in a post, as Bluesky marks them to be clickable: byte ranges of the text.
pub fn link_facets(text: &str) -> Vec<Value> {
    let mut facets = vec![];
    let mut from = 0;
    while let Some(i) = text[from..].find("http").map(|i| i + from) {
        let rest = &text[i..];
        if !(rest.starts_with("http://") || rest.starts_with("https://")) {
            from = i + 4;
            continue;
        }
        let mut end = i + rest.find(char::is_whitespace).unwrap_or(rest.len());
        // A sentence's full stop or closing bracket isn't part of the address.
        while end > i && text[..end].ends_with(['.', ',', ';', ':', '!', '?', ')', '"', '\'']) {
            end -= 1;
        }
        facets.push(json!({
            "index": { "byteStart": i, "byteEnd": end },
            "features": [{ "$type": "app.bsky.richtext.facet#link", "uri": &text[i..end] }],
        }));
        from = end.max(i + 1);
    }
    facets
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("PublishKit/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

async fn json_of(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let short: String = text.chars().take(300).collect();
        bail!("{status}: {short}");
    }
    serde_json::from_str(&text).context("the answer wasn't JSON")
}

/// `https://bsky.social`, or the person's own server.
fn bluesky_service(accounts: &Accounts) -> String {
    let service = accounts.bluesky_service.trim().trim_end_matches('/');
    if service.is_empty() {
        "https://bsky.social".into()
    } else {
        service.to_string()
    }
}

/// Posts `text` and gives the post's address.
pub async fn post(network: Network, accounts: &Accounts, text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        bail!("there is no teaser to post");
    }
    let length = text.chars().count();
    if length > network.limit() {
        bail!(
            "the teaser is {length} characters; {} takes {}",
            network.label(),
            network.limit()
        );
    }
    if !network.configured(accounts) {
        bail!(
            "{} isn't set up: add the account in ⚙ Settings",
            network.label()
        );
    }
    let client = client()?;
    match network {
        Network::Bluesky => {
            let service = bluesky_service(accounts);
            let handle = accounts.bluesky_handle.trim().trim_start_matches('@');
            let session = json_of(
                client
                    .post(format!("{service}/xrpc/com.atproto.server.createSession"))
                    .json(&json!({ "identifier": handle, "password": accounts.bluesky_password.trim() }))
                    .send()
                    .await
                    .context("Bluesky couldn't be reached")?,
            )
            .await?;
            let token = session["accessJwt"]
                .as_str()
                .context("Bluesky gave no session")?;
            let did = session["did"]
                .as_str()
                .context("Bluesky gave no account id")?;
            let record = json!({
                "$type": "app.bsky.feed.post",
                "text": text,
                "facets": link_facets(text),
                "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            });
            let created = json_of(
                client
                    .post(format!("{service}/xrpc/com.atproto.repo.createRecord"))
                    .bearer_auth(token)
                    .json(&json!({ "repo": did, "collection": "app.bsky.feed.post", "record": record }))
                    .send()
                    .await
                    .context("Bluesky couldn't be reached")?,
            )
            .await?;
            let uri = created["uri"].as_str().unwrap_or_default();
            let id = uri.rsplit('/').next().unwrap_or_default();
            Ok(format!("https://bsky.app/profile/{handle}/post/{id}"))
        }
        Network::Mastodon => {
            let instance = accounts.mastodon_instance.trim().trim_end_matches('/');
            let instance = if instance.starts_with("http") {
                instance.to_string()
            } else {
                format!("https://{instance}")
            };
            let posted = json_of(
                client
                    .post(format!("{instance}/api/v1/statuses"))
                    .bearer_auth(accounts.mastodon_token.trim())
                    .json(&json!({ "status": text }))
                    .send()
                    .await
                    .with_context(|| format!("{instance} couldn't be reached"))?,
            )
            .await?;
            Ok(posted["url"].as_str().unwrap_or_default().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_marked_by_byte_and_lose_their_full_stop() {
        let text = "Lisez l’article — https://site.org/fr/blog/été. Et https://a.b/c?d=1";
        let facets = link_facets(text);
        assert_eq!(facets.len(), 2);
        let range = |f: &Value| {
            (
                f["index"]["byteStart"].as_u64().unwrap() as usize,
                f["index"]["byteEnd"].as_u64().unwrap() as usize,
            )
        };
        let (s, e) = range(&facets[0]);
        assert_eq!(&text[s..e], "https://site.org/fr/blog/été");
        assert_eq!(
            facets[0]["features"][0]["uri"],
            "https://site.org/fr/blog/été"
        );
        let (s, e) = range(&facets[1]);
        assert_eq!(&text[s..e], "https://a.b/c?d=1");
        assert!(link_facets("no httpaddress here").is_empty());
    }

    #[tokio::test]
    async fn a_teaser_too_long_or_without_an_account_is_refused_before_sending() {
        let accounts = Accounts::default();
        let long = "x".repeat(301);
        assert!(post(Network::Bluesky, &accounts, &long)
            .await
            .unwrap_err()
            .to_string()
            .contains("301 characters"));
        assert!(post(Network::Mastodon, &accounts, "Hello")
            .await
            .unwrap_err()
            .to_string()
            .contains("isn't set up"));
        assert!(post(Network::Mastodon, &accounts, "  ").await.is_err());
    }
}

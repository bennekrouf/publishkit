//! api0, the model that runs on a Mayorana licence: one click per AI region in Settings, paid
//! for with the licence's AI credits instead of a key of one's own. The licence is the one
//! every Mayorana app on this computer shares, so a number pasted in GitAgent or JobScout is
//! already here.

use dioxus::prelude::*;
use serde_json::Value;

/// The product id api0 puts the credits down to.
pub const PRODUCT: &str = "publishkit";

/// The kind of work PublishKit's calls do, for api0's `X-Task`: a region may run another
/// model for writing than for code or chat.
pub const TASK: &str = "text";

/// api0's OpenAI-compatible endpoint.
pub const BASE_URL: &str = "https://gateway.api0.ai/v1";

/// Read before the shared licence, for a number kept out of the apps.
pub const KEY_ENV: &str = "API0_API_KEY";

pub use desktop_kit::region::{
    find as region, Region, COMING_SOON, DEFAULT as DEFAULT_REGION, REGIONS,
};

/// Tokens in a typical post: the brief, the prompt and 600–800 words back. Only for the
/// estimate.
pub const TYPICAL_POST_TOKENS: f64 = 3000.0;

/// What api0 says one of its regions costs right now.
#[derive(Clone, PartialEq, Debug)]
pub struct RegionOffer {
    pub id: String,
    /// Credits on this region cost this many times the plain rate.
    pub multiplier: f64,
    pub tokens_per_credit: i64,
}

impl RegionOffer {
    /// What a typical post costs, as api0 charges it: rounded up, at least 1.
    pub fn credits_per_post(&self) -> i64 {
        let per = self.tokens_per_credit.max(1) as f64;
        ((TYPICAL_POST_TOKENS * self.multiplier.max(0.0) / per).ceil() as i64).max(1)
    }
}

/// The regions in a `/v1/models` reply from api0.
fn parse_offers(v: &Value) -> Vec<RegionOffer> {
    let tokens_per_credit = v["tokens_per_credit"].as_i64().unwrap_or(10_000);
    v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            region(id).map(|_| RegionOffer {
                id: id.to_string(),
                multiplier: m["multiplier"].as_f64().unwrap_or(1.0),
                tokens_per_credit,
            })
        })
        .collect()
}

/// The licence api0 is paid with: a number in the environment, else the one saved on this
/// computer by any Mayorana app.
pub fn licence() -> Option<String> {
    if let Ok(v) = std::env::var(KEY_ENV) {
        if !v.trim().is_empty() {
            return Some(v.trim().to_string());
        }
    }
    // `~/Library/Application Support/mayorana/licence` on a Mac, `%APPDATA%\mayorana\licence`
    // on Windows, `~/.config/mayorana/licence` on Linux. Emptied when removed in any app.
    let path = dirs::config_dir()?.join("mayorana").join("licence");
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The regions api0 offers this licence now, with what each costs.
pub async fn regions() -> Result<Vec<RegionOffer>, String> {
    let key = licence().ok_or("api0 needs a licence")?;
    let resp = reqwest::Client::new()
        .get(format!("{BASE_URL}/models"))
        .bearer_auth(key)
        .header("X-Client-App", PRODUCT)
        // So the credits shown are those of the model that writes.
        .header("X-Task", TASK)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| network_error("api0 is unreachable", &e))?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        let msg = v["error"]["message"].as_str().unwrap_or_default();
        return Err(format!("api0 returned {status}: {msg}"));
    }
    Ok(parse_offers(&v))
}

/// Why a request got no answer, with every underlying cause. reqwest's own
/// message stops at "error sending request", which hides the one part that
/// says what to fix. Keys travel in a header, which errors never quote.
pub fn network_error(what: &str, e: &reqwest::Error) -> String {
    describe_network_failure(what, &error_chain(e), e.is_timeout())
}

/// An error and its sources, joined, without repeating what a link already said.
fn error_chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![e.to_string()];
    let mut source = e.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !parts.last().is_some_and(|last| last.contains(&text)) {
            parts.push(text);
        }
        source = cause.source();
    }
    parts.join(": ")
}

fn describe_network_failure(what: &str, chain: &str, timed_out: bool) -> String {
    let lower = chain.to_lowercase();
    let has = |markers: &[&str]| markers.iter().any(|m| lower.contains(m));
    let cause = if timed_out || has(&["timed out", "timeout"]) {
        Some("no answer within the time limit; a slow network, VPN or proxy can cause this")
    } else if has(&[
        "certificate",
        "self signed",
        "self-signed",
        "not trusted",
        "untrusted",
    ]) {
        Some(
            "this computer does not trust the certificate it was shown; a proxy that \
             inspects TLS usually needs its root certificate installed",
        )
    } else if has(&["proxy", "tunnel"]) {
        Some("the proxy refused or could not open the connection; check the proxy settings")
    } else if has(&[
        "dns error",
        "failed to lookup address",
        "name or service not known",
        "nodename nor servname",
        "no such host",
    ]) {
        Some("the server name could not be resolved; check the network or VPN")
    } else {
        None
    };
    match cause {
        Some(cause) => format!("{what}: {cause}. {chain}"),
        None => format!("{what}: {chain}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_costs_what_a_typical_post_spends_rounded_up() {
        let v = serde_json::json!({
            "object": "list",
            "tokens_per_credit": 10000,
            "data": [
                { "id": "europe", "multiplier": 1.0 },
                { "id": "usa", "multiplier": 4.0 },
                { "id": "default" }
            ]
        });
        let offers = parse_offers(&v);
        assert_eq!(offers.len(), 2, "only the regions");
        assert_eq!(offers[0].credits_per_post(), 1);
        assert_eq!(offers[1].credits_per_post(), 2);
    }

    #[test]
    fn each_kind_of_network_failure_is_named() {
        let said = |chain: &str| describe_network_failure("api0 is unreachable", chain, false);
        assert!(
            said("error sending request: invalid peer certificate: UnknownIssuer")
                .contains("does not trust the certificate")
        );
        assert!(said("error sending request: unsuccessful tunnel").contains("the proxy refused"));
        assert!(said("dns error: failed to lookup address information")
            .contains("could not be resolved"));
        assert!(said("connection timed out").contains("time limit"));
        assert_eq!(
            said("connection refused"),
            "api0 is unreachable: connection refused"
        );
    }

    /// A real refused connection: the sources reach the message, the key does not.
    #[tokio::test]
    async fn the_error_gives_the_full_cause_and_never_the_key() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/models", listener.local_addr().unwrap());
        drop(listener);
        let e = reqwest::Client::new()
            .get(&url)
            .bearer_auth("sk-very-secret")
            .send()
            .await
            .unwrap_err();
        let said = network_error("api0 is unreachable", &e);
        assert!(said.contains("error sending request"), "{said}");
        assert!(
            said.len() > e.to_string().len() + "api0 is unreachable: ".len(),
            "{said}"
        );
        assert!(!said.contains("sk-very-secret"), "{said}");
    }

    /// A real proxy turning the connection away.
    #[tokio::test]
    async fn a_proxy_that_refuses_the_tunnel_is_named() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let _ = socket.read(&mut buffer).await;
            let _ = socket.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
        });
        let e = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(proxy).unwrap())
            .build()
            .unwrap()
            .get("https://gateway.api0.ai/v1/models")
            .send()
            .await
            .unwrap_err();
        let said = network_error("api0 is unreachable", &e);
        assert!(said.contains("the proxy refused"), "{said}");
    }

    /// A real server that accepts and never answers.
    #[tokio::test]
    async fn a_server_that_never_answers_reads_as_a_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let e = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_millis(100))
            .send()
            .await
            .unwrap_err();
        let said = network_error("api0 is unreachable", &e);
        assert!(said.contains("time limit"), "{said}");
        drop(listener);
    }
}

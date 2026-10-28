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

pub use desktop_kit::region::{find as region, Region, REGIONS};

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
        .map_err(|e| format!("api0 is unreachable: {e}"))?;
    let status = resp.status();
    let v: Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        let msg = v["error"]["message"].as_str().unwrap_or_default();
        return Err(format!("api0 returned {status}: {msg}"));
    }
    Ok(parse_offers(&v))
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
}

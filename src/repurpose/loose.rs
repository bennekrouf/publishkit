//! Reading the model's reply the way it was meant rather than the way it was asked for: models
//! often hand back `{"text": …, "hashtags": […]}` or a list of lines where one string was asked
//! for, a slide as `{"title", "text"}`, a time as `"1:02"`, or the blog as its body alone.

use super::{seconds, BlogDraft};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

/// Keys that hold the text itself, most likely first.
const TEXT_KEYS: [&str; 8] = [
    "text", "body", "content", "caption", "post", "value", "message", "quote",
];
/// Keys that head the text, kept as its first line.
const HEADING_KEYS: [&str; 3] = ["title", "heading", "headline"];

/// A string, whatever shape it came in.
pub fn text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(flatten(&Value::deserialize(d)?))
}

/// A list of strings: one per item of an array, or a string's lines.
pub fn texts<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items.iter().map(flatten).collect(),
        Value::String(s) => s.lines().map(str::to_string).collect(),
        Value::Null => Vec::new(),
        Value::Object(map) => map.values().map(flatten).collect(),
        other => vec![flatten(&other)],
    })
}

/// Seconds, from a number, `"62.5"` or `"1:02"`; anything else is no time.
pub fn time<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let s = s.trim().trim_end_matches('s');
            s.parse().ok().or_else(|| seconds(s))
        }
        _ => None,
    })
}

/// The blog draft, or its body alone when that is all that came back.
pub fn blog<'de, D: Deserializer<'de>>(d: D) -> Result<Option<BlogDraft>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => None,
        value @ Value::Object(_) => BlogDraft::deserialize(value).ok(),
        other => Some(BlogDraft {
            body: flatten(&other),
            ..BlogDraft::default()
        }),
    })
}

/// Any JSON value as the text it stands for.
pub fn flatten(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => join(items.iter().map(flatten)),
        Value::Object(map) => object(map),
    }
}

/// An object as text: its heading and its text, its hashtags on a last line; failing those,
/// every value in turn.
fn object(map: &Map<String, Value>) -> String {
    let Some(body) = TEXT_KEYS.iter().find_map(|k| map.get(*k)) else {
        return join(map.values().map(flatten));
    };
    let heading = HEADING_KEYS.iter().find_map(|k| map.get(*k)).map(flatten);
    let tags = map.get("hashtags").map(hashtags).unwrap_or_default();
    join([heading.unwrap_or_default(), flatten(body), tags])
}

/// `["ai", "#rust"]` or `"#ai #rust"` as `#ai #rust`.
fn hashtags(value: &Value) -> String {
    let words = match value {
        Value::Array(items) => items.iter().map(flatten).collect::<Vec<_>>(),
        other => flatten(other)
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    };
    words
        .iter()
        .map(|w| w.trim().trim_start_matches('#'))
        .filter(|w| !w.is_empty())
        .map(|w| format!("#{}", w.replace(' ', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The parts that aren't blank, a line each.
fn join(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

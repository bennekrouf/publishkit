//! A post's front matter: the YAML (`---`) or TOML (`+++`) block at the top of its file.
//!
//! Read with a real parser, so quoting, lists and multi-line values come out right whatever
//! the framework wrote. Written by changing only the lines of the keys that change: the rest of
//! the block — comments, order, fields PublishKit knows nothing about — stays as it was.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Yaml,
    Toml,
}

impl Format {
    fn fence(self) -> &'static str {
        match self { Format::Yaml => "---", Format::Toml => "+++" }
    }
}

/// A value PublishKit writes into front matter.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Text(String),
    Bool(bool),
    List(Vec<String>),
}

pub struct Parts<'a> {
    pub format: Format,
    /// The lines between the fences.
    pub block: &'a str,
    /// Everything after the closing fence's line.
    pub body: &'a str,
}

/// Splits a file into its front matter and its body; `None` when it has no front matter.
pub fn split(content: &str) -> Option<Parts<'_>> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let format = if content.starts_with("---") {
        Format::Yaml
    } else if content.starts_with("+++") {
        Format::Toml
    } else {
        return None;
    };
    let fence = format.fence();
    let first = content.find('\n')?;
    if content[..first].trim_end() != fence {
        return None;
    }
    let start = first + 1;
    let mut pos = start;
    for line in content[start..].split_inclusive('\n') {
        if line.trim_end() == fence {
            return Some(Parts { format, block: &content[start..pos], body: &content[pos + line.len()..] });
        }
        pos += line.len();
    }
    None
}

/// The body of a post, without its front matter or the blank lines after it.
pub fn body(content: &str) -> &str {
    match split(content) {
        Some(p) => p.body.trim_start_matches(['\n', '\r']),
        None => content,
    }
}

/// The front matter's fields as JSON values; TOML dates become their text.
pub fn fields(content: &str) -> Map<String, Value> {
    let Some(p) = split(content) else { return Map::new() };
    let value = match p.format {
        Format::Yaml => serde_yaml::from_str::<Value>(p.block).ok(),
        Format::Toml => p.block.parse::<toml::Table>().ok().map(|t| toml_to_json(toml::Value::Table(t))),
    };
    match value {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

fn toml_to_json(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => i.into(),
        toml::Value::Float(f) => serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(a) => Value::Array(a.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => Value::Object(t.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect()),
    }
}

/// A field as text: strings as they are, numbers and booleans spelled out.
pub fn text(fields: &Map<String, Value>, key: &str) -> Option<String> {
    match fields.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A list field, such as tags: a list, or one comma-separated string.
pub fn list(fields: &Map<String, Value>, key: &str) -> Vec<String> {
    match fields.get(key) {
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        Some(Value::String(s)) => s.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(),
        _ => vec![],
    }
}

/// A yes/no field, such as `draft`.
pub fn flag(fields: &Map<String, Value>, key: &str) -> Option<bool> {
    match fields.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn render(format: Format, key: &str, value: &Field) -> String {
    let quote = |s: &str| serde_json::to_string(s).unwrap_or_default();
    // JSON's string escapes are valid in YAML double-quoted scalars and TOML basic strings.
    let value = match value {
        Field::Text(s) => quote(s),
        Field::Bool(b) => b.to_string(),
        Field::List(items) => format!("[{}]", items.iter().map(|s| quote(s)).collect::<Vec<_>>().join(", ")),
    };
    match format {
        Format::Yaml => format!("{key}: {value}"),
        Format::Toml => format!("{key} = {value}"),
    }
}

/// A new file: front matter with these fields, then the body.
pub fn build(format: Format, fields: &[(&str, Field)], body: &str) -> String {
    let fence = format.fence();
    let lines: Vec<String> = fields.iter().map(|(k, v)| render(format, k, v)).collect();
    format!("{fence}\n{}\n{fence}\n\n{body}", lines.join("\n"))
}

/// `content` with its body replaced, front matter kept as it is.
pub fn with_body(content: &str, body: &str) -> String {
    match split(content) {
        Some(parts) => {
            let head = &content[..content.len() - parts.body.len()];
            format!("{head}\n{}", body.trim_start_matches(['\n', '\r']))
        }
        None => body.to_string(),
    }
}

/// `content` with `key` set to `value`: the key's lines replaced, or a line added at the end of
/// the front matter. A file without front matter gets one, in `default`'s format.
pub fn set(content: &str, key: &str, value: &Field, default: Format) -> String {
    edit(content, key, Some(value), default)
}

fn edit(content: &str, key: &str, value: Option<&Field>, default: Format) -> String {
    let Some(parts) = split(content) else {
        return match value {
            Some(v) => build(default, &[(key, v.clone())], content),
            None => content.to_string(),
        };
    };
    let fence = parts.format.fence();
    let mut lines: Vec<&str> = parts.block.lines().collect();
    let new_line = value.map(|v| render(parts.format, key, v));
    match key_lines(&lines, key, parts.format) {
        Some((from, to)) => {
            lines.splice(from..to, new_line.as_deref());
        }
        None => {
            if let Some(line) = new_line.as_deref() {
                // TOML's top-level keys come before its first [table].
                let at = match parts.format {
                    Format::Toml => lines.iter().position(|l| l.trim_start().starts_with('[')).unwrap_or(lines.len()),
                    Format::Yaml => lines.len(),
                };
                lines.insert(at, line);
            }
        }
    }
    let mut out = format!("{fence}\n");
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(fence);
    out.push('\n');
    out.push_str(parts.body);
    out
}

/// The lines `from..to` holding a top-level key and its value, continuation lines included.
fn key_lines(lines: &[&str], key: &str, format: Format) -> Option<(usize, usize)> {
    match format {
        Format::Yaml => {
            let from = lines.iter().position(|l| {
                [key.to_string(), format!("\"{key}\""), format!("'{key}'")]
                    .iter()
                    .any(|k| l.strip_prefix(k.as_str()).is_some_and(|rest| rest.starts_with(':')))
            })?;
            // What follows a key, indented or as a `- item`, is its value; blank lines too
            // when more of it follows (a block scalar's paragraphs).
            let mut to = from + 1;
            let mut i = from + 1;
            while i < lines.len() {
                let l = lines[i];
                if l.trim().is_empty() {
                    i += 1;
                    continue;
                }
                if l.starts_with([' ', '\t']) || l.starts_with('-') {
                    i += 1;
                    to = i;
                } else {
                    break;
                }
            }
            Some((from, to))
        }
        Format::Toml => {
            let table_start = lines.iter().position(|l| l.trim_start().starts_with('[')).unwrap_or(lines.len());
            let from = lines[..table_start].iter().position(|l| {
                let l = l.trim_start();
                [key.to_string(), format!("\"{key}\""), format!("'{key}'")]
                    .iter()
                    .any(|k| l.strip_prefix(k.as_str()).is_some_and(|rest| rest.trim_start().starts_with('=')))
            })?;
            // A value spans lines until it parses: a multi-line array or string.
            let mut to = from + 1;
            while to < lines.len() && lines[from..to].join("\n").parse::<toml::Table>().is_err() {
                to += 1;
            }
            Some((from, to))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_fields_are_parsed_whatever_their_quoting() {
        let post = "---\ntitle: Plain title\ndescription: \"Quoted: yes\"\ntags:\n  - a\n  - b\ndraft: true\ndate: 2026-01-02\n---\n\nBody";
        let f = fields(post);
        assert_eq!(text(&f, "title").as_deref(), Some("Plain title"));
        assert_eq!(text(&f, "description").as_deref(), Some("Quoted: yes"));
        assert_eq!(list(&f, "tags"), vec!["a", "b"]);
        assert_eq!(flag(&f, "draft"), Some(true));
        assert_eq!(text(&f, "date").as_deref(), Some("2026-01-02"));
        assert_eq!(body(post), "Body");
    }

    #[test]
    fn toml_fields_are_parsed_dates_included() {
        let post = "+++\ntitle = \"Hugo post\"\ndate = 2026-01-02T10:00:00Z\ndraft = true\ntags = [\"x\"]\n+++\nBody";
        let f = fields(post);
        assert_eq!(text(&f, "title").as_deref(), Some("Hugo post"));
        assert_eq!(text(&f, "date").as_deref(), Some("2026-01-02T10:00:00Z"));
        assert_eq!(flag(&f, "draft"), Some(true));
        assert_eq!(list(&f, "tags"), vec!["x"]);
        assert_eq!(body(post), "Body");
    }

    #[test]
    fn setting_a_key_keeps_everything_else() {
        let post = "---\n# a comment\ntitle: A\ntags:\n  - a\n  - b\ncustom: kept\n---\n\nBody\n";
        let out = set(post, "tags", &Field::List(vec!["c".into()]), Format::Yaml);
        assert_eq!(out, "---\n# a comment\ntitle: A\ntags: [\"c\"]\ncustom: kept\n---\n\nBody\n");
        let out = set(&out, "draft", &Field::Bool(false), Format::Yaml);
        assert!(out.contains("custom: kept\ndraft: false\n---"));
        assert_eq!(edit(&out, "draft", None, Format::Yaml), "---\n# a comment\ntitle: A\ntags: [\"c\"]\ncustom: kept\n---\n\nBody\n");
    }

    #[test]
    fn toml_keys_are_set_before_tables_and_across_lines() {
        let post = "+++\ntitle = \"A\"\ntags = [\n  \"a\",\n]\n[extra]\nx = 1\n+++\nBody";
        let out = set(post, "tags", &Field::List(vec![]), Format::Yaml);
        assert_eq!(out, "+++\ntitle = \"A\"\ntags = []\n[extra]\nx = 1\n+++\nBody");
        let out = set(&out, "draft", &Field::Bool(true), Format::Yaml);
        assert_eq!(out, "+++\ntitle = \"A\"\ntags = []\ndraft = true\n[extra]\nx = 1\n+++\nBody");
        assert_eq!(flag(&fields(&out), "draft"), Some(true));
        assert_eq!(text(&fields(&out), "x"), None);
    }

    #[test]
    fn text_with_quotes_survives_a_round_trip() {
        for format in [Format::Yaml, Format::Toml] {
            let title = "L'été \"chaud\" \\ : # no comment";
            let post = build(format, &[("title", Field::Text(title.into()))], "Body");
            assert_eq!(text(&fields(&post), "title").as_deref(), Some(title), "{format:?}");
        }
    }

    #[test]
    fn a_new_body_keeps_the_front_matter() {
        let post = "---\ntitle: A\n---\n\nOld body";
        assert_eq!(with_body(post, "New"), "---\ntitle: A\n---\n\nNew");
        assert_eq!(body(&with_body(post, "\n\nNew")), "New");
        assert_eq!(with_body("no front matter", "New"), "New");
    }

    #[test]
    fn a_file_without_front_matter_gets_one() {
        assert_eq!(set("Body", "title", &Field::Text("A".into()), Format::Toml), "+++\ntitle = \"A\"\n+++\n\nBody");
        assert!(split("--- not a fence\n---\n").is_none());
        assert!(split("---\nno end").is_none());
    }
}

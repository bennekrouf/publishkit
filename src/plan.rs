//! What PublishKit knows of a draft that the site's files don't say: where it is in the
//! writing (an idea, a draft, in review) and when it should go out.
//!
//! Kept in `.publishkit/plan.json` at the site's root rather than in the posts' front matter, so
//! no framework's schema meets fields it doesn't know. Entries are keyed `lang/filename`; a draft
//! keeps its file name until it is published, and publishing or deleting it drops its entry.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Idea,
    #[default]
    Draft,
    Review,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::Idea, Stage::Draft, Stage::Review];

    pub fn label(self) -> &'static str {
        match self { Stage::Idea => "Idea", Stage::Draft => "Draft", Stage::Review => "Review" }
    }

    pub fn key(self) -> &'static str {
        match self { Stage::Idea => "idea", Stage::Draft => "draft", Stage::Review => "review" }
    }

    pub fn from_key(key: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|s| s.key() == key)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "is_default_stage")]
    pub stage: Stage,
    /// Local date and time, `2026-11-02T09:00`, from which the draft is published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_at: Option<String>,
}

fn is_default_stage(stage: &Stage) -> bool {
    *stage == Stage::Draft
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Plan {
    #[serde(default)]
    pub drafts: BTreeMap<String, Entry>,
}

fn path(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".publishkit").join("plan.json")
}

fn key(lang: &str, filename: &str) -> String {
    format!("{lang}/{filename}")
}

/// `2026-11-02T09:00`, or `2026-11-02 09:00`, or a plain date (at midnight).
pub fn parse_when(text: &str) -> Option<chrono::NaiveDateTime> {
    let text = text.trim();
    ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .find_map(|f| chrono::NaiveDateTime::parse_from_str(text, f).ok())
        .or_else(|| chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)))
}

/// The way `publish_at` is stored, and an `<input type="datetime-local">` takes it.
pub fn format_when(when: chrono::NaiveDateTime) -> String {
    when.format("%Y-%m-%dT%H:%M").to_string()
}

impl Plan {
    pub fn load(project_path: &str) -> Plan {
        std::fs::read_to_string(path(project_path))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, project_path: &str) -> std::io::Result<()> {
        let path = path(project_path);
        if self.drafts.is_empty() && !path.exists() {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    pub fn get(&self, lang: &str, filename: &str) -> Entry {
        self.drafts.get(&key(lang, filename)).cloned().unwrap_or_default()
    }

    /// Sets a draft's entry; one with nothing to say is dropped.
    pub fn set(&mut self, lang: &str, filename: &str, entry: Entry) {
        if entry == Entry::default() {
            self.drafts.remove(&key(lang, filename));
        } else {
            self.drafts.insert(key(lang, filename), entry);
        }
    }

    pub fn remove(&mut self, lang: &str, filename: &str) {
        self.drafts.remove(&key(lang, filename));
    }

    /// The drafts whose time has come, as `(lang, filename)`, earliest first.
    pub fn due(&self, now: chrono::NaiveDateTime) -> Vec<(String, String)> {
        let mut due: Vec<(chrono::NaiveDateTime, String, String)> = self
            .drafts
            .iter()
            .filter_map(|(k, e)| {
                let when = parse_when(e.publish_at.as_deref()?)?;
                let (lang, filename) = k.split_once('/')?;
                (when <= now).then(|| (when, lang.to_string(), filename.to_string()))
            })
            .collect();
        due.sort();
        due.into_iter().map(|(_, l, f)| (l, f)).collect()
    }
}

/// Changes one draft's entry in the site's plan, on disk.
pub fn update(project_path: &str, lang: &str, filename: &str, change: impl FnOnce(&mut Entry)) -> std::io::Result<()> {
    let mut plan = Plan::load(project_path);
    let mut entry = plan.get(lang, filename);
    change(&mut entry);
    plan.set(lang, filename, entry);
    plan.save(project_path)
}

/// Drops a draft from the site's plan, if it is there.
pub fn forget(project_path: &str, lang: &str, filename: &str) {
    let mut plan = Plan::load(project_path);
    if plan.drafts.contains_key(&key(lang, filename)) {
        plan.remove(lang, filename);
        let _ = plan.save(project_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_with_nothing_to_say_are_not_kept() {
        let mut plan = Plan::default();
        plan.set("en", "a.md", Entry { stage: Stage::Review, publish_at: None });
        plan.set("en", "b.md", Entry::default());
        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(serde_json::to_string(&plan).unwrap(), r#"{"drafts":{"en/a.md":{"stage":"review"}}}"#);
        assert_eq!(plan.get("en", "b.md").stage, Stage::Draft);
    }

    #[test]
    fn what_is_due_comes_earliest_first() {
        let mut plan = Plan::default();
        let at = |s: &str| Entry { publish_at: Some(s.into()), ..Default::default() };
        plan.set("fr", "late.md", at("2026-11-02T10:00"));
        plan.set("en", "early.md", at("2026-11-02 09:00"));
        plan.set("en", "future.md", at("2026-12-01"));
        plan.set("en", "garbled.md", at("next tuesday"));
        let now = parse_when("2026-11-02T10:00").unwrap();
        assert_eq!(plan.due(now), [("en".to_string(), "early.md".to_string()), ("fr".into(), "late.md".into())]);
    }
}

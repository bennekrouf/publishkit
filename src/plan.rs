//! What PublishKit knows of a draft that the site's files don't say: where it is in the
//! writing (an idea, a draft, in review) and when it should go out.
//!
//! Kept in `.publishkit/plan.json` at the site's root rather than in the posts' front matter, so
//! no framework's schema meets fields it doesn't know. Entries are keyed `lang/filename`; a draft
//! keeps its file name until it is published, and publishing or deleting it drops its entry.
//! LinkedIn and X drafts have entries of their own: PublishKit doesn't post to those networks, so
//! their date is a reminder on the calendar, never something done on their own.

use crate::state::Channel;
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
        match self {
            Stage::Idea => "Idea",
            Stage::Draft => "Draft",
            Stage::Review => "Review",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Stage::Idea => "idea",
            Stage::Draft => "draft",
            Stage::Review => "review",
        }
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
    /// LinkedIn drafts, keyed the same way. Only `publish_at` is used: they have no board.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub linkedin: BTreeMap<String, Entry>,
    /// X drafts, as LinkedIn's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub x: BTreeMap<String, Entry>,
    /// Instagram drafts, as LinkedIn's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub instagram: BTreeMap<String, Entry>,
    /// TikTok drafts, as LinkedIn's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tiktok: BTreeMap<String, Entry>,
}

fn path(project_path: &str) -> PathBuf {
    Path::new(project_path)
        .join(".publishkit")
        .join("plan.json")
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
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
}

/// The way `publish_at` is stored, and an `<input type="datetime-local">` takes it.
pub fn format_when(when: chrono::NaiveDateTime) -> String {
    when.format("%Y-%m-%dT%H:%M").to_string()
}

/// When a post went out (or goes out): a day, and the time of day when one is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct When {
    pub day: chrono::NaiveDate,
    pub time: Option<chrono::NaiveTime>,
}

impl When {
    /// `2026-10-16`, `2026-10-16T09:00`, `2026-10-16 09:00` or a full timestamp with its
    /// offset, `2026-10-16T09:00:00+02:00`, read as the time written there.
    pub fn parse(text: &str) -> Option<When> {
        let text = text.trim();
        if let Ok(stamp) = chrono::DateTime::parse_from_rfc3339(text) {
            let local = stamp.naive_local();
            return Some(When {
                day: local.date(),
                time: Some(local.time()),
            });
        }
        if text.len() > 10 {
            let at = parse_when(text)?;
            return Some(When {
                day: at.date(),
                time: Some(at.time()),
            });
        }
        let day = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
        Some(When { day, time: None })
    }

    /// A post's date from its front matter: `date`, with the time from `time:` when the date
    /// has none, or from a `publish_at` written by hand on the same day.
    pub fn of_post(fields: &serde_json::Map<String, serde_json::Value>) -> Option<When> {
        let text = |key: &str| crate::frontmatter::text(fields, key);
        let mut when = When::parse(&text("date")?)?;
        if when.time.is_none() {
            when.time = text("time")
                .and_then(|t| {
                    ["%H:%M", "%H:%M:%S"]
                        .iter()
                        .find_map(|f| chrono::NaiveTime::parse_from_str(t.trim(), f).ok())
                })
                .or_else(|| {
                    text("publish_at")
                        .and_then(|t| When::parse(&t))
                        .filter(|w| w.day == when.day)
                        .and_then(|w| w.time)
                });
        }
        Some(when)
    }

    /// `09:00`, when the time is known.
    pub fn time_label(&self) -> Option<String> {
        self.time.map(|t| t.format("%H:%M").to_string())
    }

    /// As a LinkedIn or X post's `date` keeps it: `2026-10-16T09:00`, or the plain day.
    pub fn social_date(&self) -> String {
        match self.time {
            Some(t) => format_when(self.day.and_time(t)),
            None => self.day.format("%Y-%m-%d").to_string(),
        }
    }

    /// As a blog post's `date` is written: a full timestamp with this computer's offset when
    /// there is a time or the post's date had one (`previous`), which every framework reads;
    /// else the plain day.
    pub fn blog_date(&self, previous: &str) -> String {
        use chrono::TimeZone;
        if self.time.is_none() && !previous.contains('T') {
            return self.day.format("%Y-%m-%d").to_string();
        }
        let at = self.day.and_time(self.time.unwrap_or_default());
        match chrono::Local.from_local_datetime(&at).earliest() {
            Some(local) => local.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
            None => at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        }
    }
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
        if self.drafts.is_empty() && self.linkedin.is_empty() && self.x.is_empty() && !path.exists()
        {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    fn social(&self, channel: Channel) -> &BTreeMap<String, Entry> {
        match channel {
            Channel::LinkedIn => &self.linkedin,
            Channel::X => &self.x,
            Channel::Instagram => &self.instagram,
            Channel::TikTok => &self.tiktok,
        }
    }

    fn social_mut(&mut self, channel: Channel) -> &mut BTreeMap<String, Entry> {
        match channel {
            Channel::LinkedIn => &mut self.linkedin,
            Channel::X => &mut self.x,
            Channel::Instagram => &mut self.instagram,
            Channel::TikTok => &mut self.tiktok,
        }
    }

    /// When a network's draft is planned to go out: `2026-11-02T09:00`.
    pub fn social_at(&self, channel: Channel, lang: &str, filename: &str) -> Option<String> {
        self.social(channel)
            .get(&key(lang, filename))
            .and_then(|e| e.publish_at.clone())
    }

    pub fn get(&self, lang: &str, filename: &str) -> Entry {
        self.drafts
            .get(&key(lang, filename))
            .cloned()
            .unwrap_or_default()
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
pub fn update(
    project_path: &str,
    lang: &str,
    filename: &str,
    change: impl FnOnce(&mut Entry),
) -> std::io::Result<()> {
    let mut plan = Plan::load(project_path);
    let mut entry = plan.get(lang, filename);
    change(&mut entry);
    plan.set(lang, filename, entry);
    plan.save(project_path)
}

/// When a network's draft is planned to go out: `2026-11-02T09:00`.
pub fn social_at(
    project_path: &str,
    channel: Channel,
    lang: &str,
    filename: &str,
) -> Option<String> {
    Plan::load(project_path).social_at(channel, lang, filename)
}

/// Plans a LinkedIn or X draft for `when`, or drops its date with `None`.
pub fn plan_social(
    project_path: &str,
    channel: Channel,
    lang: &str,
    filename: &str,
    when: Option<String>,
) -> std::io::Result<()> {
    let mut plan = Plan::load(project_path);
    match when {
        Some(at) => {
            plan.social_mut(channel).insert(
                key(lang, filename),
                Entry {
                    publish_at: Some(at),
                    ..Default::default()
                },
            );
        }
        None => {
            plan.social_mut(channel).remove(&key(lang, filename));
        }
    }
    plan.save(project_path)
}

/// Drops a LinkedIn or X draft from the site's plan, if it is there.
pub fn forget_social(project_path: &str, channel: Channel, lang: &str, filename: &str) {
    let mut plan = Plan::load(project_path);
    if plan
        .social_mut(channel)
        .remove(&key(lang, filename))
        .is_some()
    {
        let _ = plan.save(project_path);
    }
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
        plan.set(
            "en",
            "a.md",
            Entry {
                stage: Stage::Review,
                publish_at: None,
            },
        );
        plan.set("en", "b.md", Entry::default());
        assert_eq!(plan.drafts.len(), 1);
        assert_eq!(
            serde_json::to_string(&plan).unwrap(),
            r#"{"drafts":{"en/a.md":{"stage":"review"}}}"#
        );
        assert_eq!(plan.get("en", "b.md").stage, Stage::Draft);
    }

    #[test]
    fn a_post_date_keeps_its_time_wherever_it_is_written() {
        let when = |front: &str| {
            When::of_post(&crate::frontmatter::fields(&format!(
                "---\n{front}\n---\nBody"
            )))
            .map(|w| (w.day.to_string(), w.time_label()))
        };
        let at = |d: &str, t: Option<&str>| Some((d.to_string(), t.map(String::from)));
        assert_eq!(when("date: \"2026-10-16\""), at("2026-10-16", None));
        assert_eq!(
            when("date: \"2026-10-16T09:00\""),
            at("2026-10-16", Some("09:00"))
        );
        assert_eq!(
            when("date: 2026-10-16T09:00:00+02:00"),
            at("2026-10-16", Some("09:00"))
        );
        assert_eq!(
            when("date: \"2026-10-16\"\ntime: \"09:00\""),
            at("2026-10-16", Some("09:00"))
        );
        // Written by hand on a post saved after it went out.
        assert_eq!(
            when("date: \"2026-10-16\"\npublish_at: \"2026-10-16T09:00\""),
            at("2026-10-16", Some("09:00"))
        );
        // Another day's publish_at says nothing of this one.
        assert_eq!(
            when("date: \"2026-10-16\"\npublish_at: \"2026-10-20T09:00\""),
            at("2026-10-16", None)
        );
        assert_eq!(when("title: x"), None);
    }

    #[test]
    fn a_date_is_written_the_way_each_kind_of_post_keeps_it() {
        let w = When::parse("2026-10-16T09:00").unwrap();
        assert_eq!(w.social_date(), "2026-10-16T09:00");
        assert!(w.blog_date("").starts_with("2026-10-16T09:00:00"));
        let day = When::parse("2026-10-16").unwrap();
        assert_eq!(day.social_date(), "2026-10-16");
        assert_eq!(day.blog_date("2026-01-01"), "2026-10-16");
        assert!(day
            .blog_date("2026-01-01T08:00:00Z")
            .starts_with("2026-10-16T00:00:00"));
    }

    #[test]
    fn linkedin_dates_are_kept_apart_from_the_blog_schedule() {
        let mut plan = Plan::default();
        plan.linkedin.insert(
            "fr/a.md".into(),
            Entry {
                publish_at: Some("2026-11-02T09:00".into()),
                ..Default::default()
            },
        );
        assert!(plan.due(parse_when("2027-01-01").unwrap()).is_empty());
        assert_eq!(plan.get("fr", "a.md"), Entry::default());
    }

    #[test]
    fn what_is_due_comes_earliest_first() {
        let mut plan = Plan::default();
        let at = |s: &str| Entry {
            publish_at: Some(s.into()),
            ..Default::default()
        };
        plan.set("fr", "late.md", at("2026-11-02T10:00"));
        plan.set("en", "early.md", at("2026-11-02 09:00"));
        plan.set("en", "future.md", at("2026-12-01"));
        plan.set("en", "garbled.md", at("next tuesday"));
        let now = parse_when("2026-11-02T10:00").unwrap();
        assert_eq!(
            plan.due(now),
            [
                ("en".to_string(), "early.md".to_string()),
                ("fr".into(), "late.md".into())
            ]
        );
    }
}

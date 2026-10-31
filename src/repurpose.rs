//! Repurpose: one finished recording — a PolishCast video or a TrackCut audio file — and its
//! transcript turned, in one call to the model, into short clips to cut, carousel slides, a post
//! for each network and a blog draft, all queued as drafts the user edits or throws away.
//!
//! Nothing is cut or drawn here: a clip is a TikTok (or Instagram) draft saying where in the
//! recording it starts and ends, and the carousel's slides are text on its Instagram draft. The
//! transcript is required: PublishKit doesn't listen to the recording. A timed one (`.srt`,
//! `.vtt`) gives clips their times; a plain one only the passage to cut.

use crate::api;
use crate::frontmatter::Field;
use crate::plan::{self, Stage};
use crate::state::{Channel, LlmProvider, ProjectConfig};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
mod window;
pub use window::*;

/// What PolishCast and TrackCut export, and what else a recording usually comes as.
pub const MEDIA_EXTENSIONS: [&str; 11] = [
    "mp4", "mov", "m4v", "webm", "mkv", "mp3", "wav", "m4a", "aac", "flac", "ogg",
];
pub const TRANSCRIPT_EXTENSIONS: [&str; 4] = ["srt", "vtt", "txt", "md"];
pub const MIN_CLIPS: usize = 3;
pub const MAX_CLIPS: usize = 8;
pub const DEFAULT_CLIPS: usize = 5;

/// Shorter than this, a clip is a sound bite, not a short.
const MIN_CLIP_SECS: f64 = 5.0;
/// The longest Reels and Shorts take, TikTok's being longer.
const MAX_CLIP_SECS: f64 = 180.0;
/// About 15,000 tokens of transcript: an hour of talk. A longer one is cut, and said so.
const MAX_TRANSCRIPT_CHARS: usize = 60_000;

/// One line of a timed transcript, in seconds of the recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A recording's words: timed cues from subtitles, or plain text.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Transcript {
    pub cues: Vec<Cue>,
    /// The text of an untimed transcript; empty when there are cues.
    pub plain: String,
}

impl Transcript {
    /// Subtitles (SRT or WebVTT) when it has `-->` timings, else plain text.
    pub fn parse(raw: &str) -> Transcript {
        let raw = raw.trim_start_matches('\u{feff}');
        let cues = cues(raw);
        if cues.is_empty() {
            Transcript {
                cues,
                plain: raw.trim().to_string(),
            }
        } else {
            Transcript {
                cues,
                plain: String::new(),
            }
        }
    }

    pub fn read(path: &Path) -> Result<Transcript> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("couldn't read the transcript {}", path.display()))?;
        Ok(Transcript::parse(&raw))
    }

    pub fn timed(&self) -> bool {
        !self.cues.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.cues.is_empty() && self.plain.is_empty()
    }

    /// Where the last words end, for a timed one.
    pub fn end(&self) -> Option<f64> {
        self.cues.iter().map(|c| c.end).reduce(f64::max)
    }

    /// As the model reads it: `[1:02] words` per cue, or the plain text; cut to `max` characters,
    /// with whether it was.
    fn for_prompt(&self, max: usize) -> (String, bool) {
        let full = if self.timed() {
            self.cues
                .iter()
                .map(|c| format!("[{}] {}", clock(c.start), c.text))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            self.plain.clone()
        };
        if full.chars().count() <= max {
            return (full, false);
        }
        (full.chars().take(max).collect(), true)
    }
}

/// The cues of SRT or WebVTT subtitles: each block's `start --> end` line and the text under it,
/// tags such as `<i>` taken out.
fn cues(raw: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    let normalised = raw.replace("\r\n", "\n");
    for block in normalised.split("\n\n") {
        let mut lines = block.lines().map(str::trim).filter(|l| !l.is_empty());
        let Some(timing) = lines.by_ref().find(|l| l.contains("-->")) else {
            continue;
        };
        let Some((from, to)) = timing.split_once("-->") else {
            continue;
        };
        // WebVTT puts cue settings after the end time: `00:01.000 --> 00:04.000 align:start`.
        let to = to.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (seconds(from.trim()), seconds(to)) else {
            continue;
        };
        let text = lines.map(strip_tags).collect::<Vec<_>>().join(" ");
        if !text.trim().is_empty() && end >= start {
            cues.push(Cue {
                start,
                end,
                text: text.trim().to_string(),
            });
        }
    }
    cues
}

/// `01:02:03,450`, `02:03.450` or `2:03` in seconds.
fn seconds(stamp: &str) -> Option<f64> {
    let stamp = stamp.replace(',', ".");
    let mut total = 0.0;
    let parts: Vec<&str> = stamp.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    for part in parts {
        let value: f64 = part.parse().ok()?;
        if value < 0.0 {
            return None;
        }
        total = total * 60.0 + value;
    }
    Some(total)
}

fn strip_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_tag = false;
    for c in line.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Seconds as a clock: `1:02`, or `1:02:03` past the hour.
pub fn clock(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The transcript next to a recording, by the same name: `talk.srt`, `talk.vtt` or `talk.txt`
/// beside `talk.mp4`. Timed ones first.
pub fn sidecar(media: &Path) -> Option<PathBuf> {
    TRANSCRIPT_EXTENSIONS
        .iter()
        .map(|ext| media.with_extension(ext))
        .find(|p| p.is_file() && p != media)
}

pub fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["mp4", "mov", "m4v", "webm", "mkv"].contains(&e.to_lowercase().as_str()))
}

/// The blog post to write from the recording, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlogKind {
    #[default]
    Draft,
    Outline,
    None,
}

impl BlogKind {
    pub const ALL: [BlogKind; 3] = [BlogKind::Draft, BlogKind::Outline, BlogKind::None];

    pub fn key(self) -> &'static str {
        match self {
            BlogKind::Draft => "draft",
            BlogKind::Outline => "outline",
            BlogKind::None => "none",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BlogKind::Draft => "Full draft",
            BlogKind::Outline => "Outline",
            BlogKind::None => "No blog post",
        }
    }

    pub fn from_key(key: &str) -> Option<BlogKind> {
        BlogKind::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// What to make, from what.
#[derive(Clone, Debug)]
pub struct Request {
    pub media: PathBuf,
    pub transcript: Transcript,
    pub lang: String,
    /// What the recording is called or is about, when the user says.
    pub title: String,
    pub clips: usize,
    pub blog: BlogKind,
    /// The networks to write for: the site's, unless fewer are asked for.
    pub networks: Vec<Channel>,
}

impl Request {
    /// Where the clips go: TikTok, else Instagram as reels, else nowhere.
    fn clip_channel(&self) -> Option<Channel> {
        [Channel::TikTok, Channel::Instagram]
            .into_iter()
            .find(|c| self.networks.contains(c))
    }

    fn media_name(&self) -> String {
        self.media
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// What the model makes of the recording.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Plan {
    pub title: String,
    pub summary: String,
    pub clips: Vec<Clip>,
    pub slides: Vec<String>,
    pub linkedin: String,
    pub x: String,
    pub instagram: String,
    pub blog: Option<BlogDraft>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Clip {
    pub start: Option<f64>,
    pub end: Option<f64>,
    /// The words it opens on, for an untimed transcript: the passage to find and cut.
    pub quote: String,
    pub hook: String,
    pub caption: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct BlogDraft {
    pub title: String,
    pub summary: String,
    pub body: String,
}

/// The model's plan for the recording, fitted to each network's limits.
pub async fn generate(
    request: &Request,
    project_config: &ProjectConfig,
    provider: &LlmProvider,
    api_key: &str,
) -> Result<(Plan, Vec<String>)> {
    if request.transcript.is_empty() {
        bail!("the transcript is empty");
    }
    let (system, user, cut) = prompt(request, project_config);
    let raw = api::call(provider, &system, &user, api_key).await?;
    let plan = parse_plan(&raw)?;
    let (plan, mut notes) = tidy(plan, request);
    if cut {
        notes.insert(
            0,
            format!(
                "the transcript is long: only its first {MAX_TRANSCRIPT_CHARS} characters were read"
            ),
        );
    }
    Ok((plan, notes))
}

fn prompt(request: &Request, cfg: &ProjectConfig) -> (String, String, bool) {
    let write_in = api::write_in(&request.lang);
    let brief = cfg.system_prompt.trim();
    let system = format!(
        "{brief}\n\nYou repurpose one long recording into short-form content for social networks and \
         a blog. Stay true to what is said in it: quote it, don't invent facts. {write_in}"
    );
    let (transcript, cut) = request.transcript.for_prompt(MAX_TRANSCRIPT_CHARS);
    let timed = request.transcript.timed();
    let kind = if is_video(&request.media) {
        "video"
    } else {
        "audio recording"
    };
    let about = if request.title.trim().is_empty() {
        String::new()
    } else {
        format!("It is about: {}\n", request.title.trim())
    };
    let clip_rule = if timed {
        format!(
            "\"clips\": {n} moments that stand on their own as vertical shorts, each {lo:.0}-90 \
             seconds, not overlapping, the strongest first: {{\"start\": seconds, \"end\": seconds, \
             \"hook\": the on-screen hook for its first 2 seconds (max 8 words), \"caption\": its \
             TikTok caption, a hook first line, max 300 characters, 3-5 hashtags at the end}}. \
             Start and end on the transcript's timestamps, at the start and end of a sentence.",
            n = request.clips,
            lo = MIN_CLIP_SECS * 3.0,
        )
    } else {
        format!(
            "\"clips\": {n} passages that stand on their own as vertical shorts of 15-90 seconds, \
             the strongest first: {{\"quote\": the first 8-12 words of the passage, exactly as in \
             the transcript, \"hook\": the on-screen hook for its first 2 seconds (max 8 words), \
             \"caption\": its TikTok caption, a hook first line, max 300 characters, 3-5 hashtags \
             at the end}}.",
            n = request.clips
        )
    };
    let networks = &request.networks;
    let wants = |c: Channel| networks.contains(&c);
    let mut fields = vec![
        "\"title\": what the recording is about, as a headline".to_string(),
        "\"summary\": one or two sentences".to_string(),
    ];
    if request.clip_channel().is_some() {
        fields.push(clip_rule);
    }
    if wants(Channel::Instagram) {
        fields.push(
            "\"slides\": 5-8 carousel slides / quote cards, in order: a hook slide, then one idea or \
             quote each (max 25 words per slide), then a closing call to action"
                .into(),
        );
        fields.push(format!(
            "\"instagram\": the carousel's caption: a first line that stops the scroll, short lines, \
             at most {} hashtags on a last line of their own, no links, max 2000 characters",
            Channel::Instagram.max_hashtags().unwrap_or(5)
        ));
    }
    if wants(Channel::LinkedIn) {
        fields.push(format!(
            "\"linkedin\": a LinkedIn post: a strong first line, 4-8 short paragraphs, the key \
             takeaways, max 3 hashtags, max {} characters",
            Channel::LinkedIn.limit().min(2500)
        ));
    }
    if wants(Channel::X) {
        fields.push(format!(
            "\"x\": one post on X, a single sharp idea from the recording, max 2 hashtags, max {} \
             characters in all",
            Channel::X.limit() - 10
        ));
    }
    match request.blog {
        BlogKind::Draft => fields.push(
            "\"blog\": {\"title\", \"summary\": one or two sentences, \"body\": the markdown body, \
             no front matter and no title heading: intro, 3-5 ## sections, conclusion, 700-1000 \
             words, in the site's voice}"
                .into(),
        ),
        BlogKind::Outline => fields.push(
            "\"blog\": {\"title\", \"summary\": one or two sentences, \"body\": a markdown outline, \
             no front matter and no title heading: the angle in one paragraph, then 3-5 ## \
             sections, each with 2-4 bullet points of what to say and the transcript quotes to use}"
                .into(),
        ),
        BlogKind::None => {}
    }
    let user = format!(
        "This is the transcript of a {kind}.\n{about}\
         Reply with ONLY a JSON object (no markdown fences, no extra text) with these fields:\n- {}\n\n\
         Transcript{}:\n{transcript}",
        fields.join("\n- "),
        if timed { " (each line starts at [minutes:seconds])" } else { "" },
    );
    (system, user, cut)
}

/// The JSON object in the model's reply, fences or a sentence around it ignored.
fn parse_plan(raw: &str) -> Result<Plan> {
    let (Some(from), Some(to)) = (raw.find('{'), raw.rfind('}')) else {
        bail!("the model's reply had no JSON in it");
    };
    if to < from {
        bail!("the model's reply had no JSON in it");
    }
    serde_json::from_str(&raw[from..=to]).context("the model's reply wasn't the JSON asked for")
}

/// The plan within what the networks take, and notes on what was changed.
pub fn tidy(mut plan: Plan, request: &Request) -> (Plan, Vec<String>) {
    let mut notes = Vec::new();
    let end = request.transcript.end();
    let timed = request.transcript.timed();
    let mut clips: Vec<Clip> = Vec::new();
    for mut clip in std::mem::take(&mut plan.clips) {
        if clip.caption.trim().is_empty() {
            continue;
        }
        if timed {
            let (Some(mut start), Some(mut stop)) = (clip.start, clip.end) else {
                continue;
            };
            if !start.is_finite() || !stop.is_finite() {
                continue;
            }
            start = start.max(0.0);
            if let Some(end) = end {
                stop = stop.min(end);
            }
            stop = stop.min(start + MAX_CLIP_SECS);
            if stop - start < MIN_CLIP_SECS {
                continue;
            }
            // Two clips of the same moment are one.
            let overlaps = clips.iter().any(|c| {
                let (s, e) = (c.start.unwrap_or(0.0), c.end.unwrap_or(0.0));
                start < e && s < stop && (stop.min(e) - start.max(s)) > (stop - start) / 2.0
            });
            if overlaps {
                continue;
            }
            clip.start = Some(start);
            clip.end = Some(stop);
        } else {
            clip.start = None;
            clip.end = None;
            if clip.quote.trim().is_empty() {
                continue;
            }
        }
        clip.caption = fit_channel(
            &clip.caption,
            request.clip_channel().unwrap_or(Channel::TikTok),
            &mut notes,
        );
        clips.push(clip);
        if clips.len() == request.clips {
            break;
        }
    }
    if request.clip_channel().is_some() && clips.len() < MIN_CLIPS.min(request.clips) {
        notes.push(format!(
            "only {} usable clip{} came back: the recording may be short, or the transcript thin",
            clips.len(),
            if clips.len() == 1 { "" } else { "s" }
        ));
    }
    plan.clips = clips;
    plan.slides = std::mem::take(&mut plan.slides)
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(10)
        .collect();
    plan.linkedin = fit_channel(&plan.linkedin, Channel::LinkedIn, &mut notes);
    plan.x = fit_channel(&plan.x, Channel::X, &mut notes);
    plan.instagram = fit_channel(&plan.instagram, Channel::Instagram, &mut notes);
    if request.blog == BlogKind::None {
        plan.blog = None;
    }
    plan.blog = plan.blog.filter(|b| !b.body.trim().is_empty());
    (plan, notes)
}

/// `text` within `channel`'s length and hashtags, with a note when it had to be cut.
fn fit_channel(text: &str, channel: Channel, notes: &mut Vec<String>) -> String {
    let mut text = text.trim().to_string();
    if let Some(max) = channel.max_hashtags() {
        let had = crate::state::hashtags(&text);
        if had > max {
            text = keep_hashtags(&text, max);
            notes.push(format!(
                "the {} text had {had} hashtags; the first {max} were kept",
                channel.label()
            ));
        }
    }
    let limit = channel.limit();
    if text.chars().count() > limit {
        text = fit(&text, limit);
        notes.push(format!(
            "the {} text was longer than {limit} characters and was shortened",
            channel.label()
        ));
    }
    text
}

/// `text` without the hashtags past the first `max`; a line left empty goes too.
fn keep_hashtags(text: &str, max: usize) -> String {
    let mut seen = 0;
    let mut lines = Vec::new();
    for line in text.lines() {
        let mut kept = Vec::new();
        let mut dropped = false;
        for word in line.split(' ') {
            let is_tag = word
                .strip_prefix('#')
                .and_then(|r| r.chars().next())
                .is_some_and(char::is_alphanumeric);
            if is_tag {
                seen += 1;
                if seen > max {
                    dropped = true;
                    continue;
                }
            }
            kept.push(word);
        }
        let line = kept.join(" ");
        if dropped && line.trim().is_empty() {
            continue;
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

/// `text` cut to at most `limit` characters: at the end of a sentence or a line when one is
/// past half of it, else at a word, with an ellipsis.
fn fit(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit).collect();
    let sentence = head
        .char_indices()
        .filter(|(i, c)| {
            matches!(c, '.' | '!' | '?' | '\n')
                && head[i + c.len_utf8()..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
        })
        .map(|(i, c)| i + c.len_utf8())
        .next_back();
    if let Some(end) = sentence.filter(|&e| head[..e].chars().count() > limit / 2) {
        return head[..end].trim_end().to_string();
    }
    let room: String = text.chars().take(limit.saturating_sub(1)).collect();
    let cut = room.rfind(char::is_whitespace).unwrap_or(room.len());
    format!("{}…", room[..cut].trim_end())
}

/// One draft the plan was saved as.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// `None` for the blog.
    pub channel: Option<Channel>,
    pub path: PathBuf,
    pub topic: String,
    /// Which of the plan's clips it is, for a clip.
    pub clip: Option<usize>,
}

impl Saved {
    pub fn kind(&self) -> &'static str {
        self.channel.map_or("blog", Channel::key)
    }

    pub fn filename(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// The plan queued as drafts: one per clip, the carousel, a post per network and the blog post.
/// What was already saved stays when a later one fails, and the error says how far it got.
pub fn save(
    project: &str,
    cfg: &ProjectConfig,
    request: &Request,
    plan: &Plan,
) -> Result<Vec<Saved>> {
    let mut saved = Vec::new();
    let result = save_into(project, cfg, request, plan, &mut saved);
    match result {
        Ok(()) => Ok(saved),
        Err(e) if saved.is_empty() => Err(e),
        Err(e) => Err(e.context(format!(
            "{} draft{} queued before this",
            saved.len(),
            if saved.len() == 1 { " was" } else { "s were" }
        ))),
    }
}

fn save_into(
    project: &str,
    cfg: &ProjectConfig,
    request: &Request,
    plan: &Plan,
    saved: &mut Vec<Saved>,
) -> Result<()> {
    let source = request.media.to_string_lossy().into_owned();
    let name = request.media_name();
    let lang = &request.lang;
    let from = || ("repurposed_from", Field::Text(source.clone()));
    let mut social = |channel: Channel,
                      topic: String,
                      body: &str,
                      extra: Vec<(&str, Field)>,
                      clip: Option<usize>| {
        if body.trim().is_empty() {
            return Ok(());
        }
        let mut fields = vec![from()];
        fields.extend(extra);
        let path = api::save_social_with(project, cfg, channel, &topic, lang, body, &fields)?;
        saved.push(Saved {
            channel: Some(channel),
            path: PathBuf::from(path),
            topic,
            clip,
        });
        anyhow::Ok(())
    };
    if let Some(channel) = request.clip_channel() {
        for (i, clip) in plan.clips.iter().enumerate() {
            let at = match (clip.start, clip.end) {
                (Some(s), Some(e)) => format!("{}–{}", clock(s), clock(e)),
                _ => format!("“{}”", short(&clip.quote, 40)),
            };
            let topic = format!("Clip {} · {at} · {}", i + 1, short(&clip.hook, 50));
            let mut extra = vec![("hook", Field::Text(clip.hook.trim().to_string()))];
            if let (Some(s), Some(e)) = (clip.start, clip.end) {
                extra.push(("clip_start", Field::Text(format!("{s:.1}"))));
                extra.push(("clip_end", Field::Text(format!("{e:.1}"))));
            } else {
                extra.push(("clip_quote", Field::Text(clip.quote.trim().to_string())));
            }
            social(channel, topic, &clip.caption, extra, Some(i))?;
        }
    }
    if request.networks.contains(&Channel::Instagram) {
        let topic = format!("Carousel · {}", short(&plan.title, 60));
        let extra = if plan.slides.is_empty() {
            vec![]
        } else {
            vec![("slides", Field::List(plan.slides.clone()))]
        };
        social(Channel::Instagram, topic, &plan.instagram, extra, None)?;
    }
    for channel in [Channel::LinkedIn, Channel::X] {
        if !request.networks.contains(&channel) {
            continue;
        }
        let body = if channel == Channel::LinkedIn {
            &plan.linkedin
        } else {
            &plan.x
        };
        social(channel, short(&plan.title, 60), body, vec![], None)?;
    }
    if let Some(blog) = plan.blog.as_ref().filter(|_| cfg.blog) {
        let title = Some(blog.title.trim())
            .filter(|t| !t.is_empty())
            .unwrap_or(plan.title.trim());
        let summary = Some(blog.summary.trim())
            .filter(|t| !t.is_empty())
            .unwrap_or(plan.summary.trim());
        let path = api::save_to_queue(project, title, summary, lang, blog.body.trim(), cfg)
            .with_context(|| format!("the blog draft from {name}"))?;
        let path = PathBuf::from(path);
        // An outline is an idea on the board until it is written.
        if request.blog == BlogKind::Outline {
            let (queue, _) = cfg.resolve_paths(project, lang);
            if let Ok(rel) = path.strip_prefix(&queue) {
                let filename = rel.to_string_lossy().replace('\\', "/");
                let _ = plan::update(project, lang, &filename, |e| e.stage = Stage::Idea);
            }
        }
        saved.push(Saved {
            channel: None,
            path,
            topic: title.to_string(),
            clip: None,
        });
    }
    Ok(())
}

/// `text` on one line, cut at a word to at most `max` characters.
fn short(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let room: String = line.chars().take(max.saturating_sub(1)).collect();
    let cut = room
        .rfind(' ')
        .filter(|&i| i > max / 3)
        .unwrap_or(room.len());
    format!("{}…", &room[..cut])
}

/// What a repurposed draft says about where it came from, read from its front matter.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Origin {
    /// The recording, as an absolute path.
    pub source: String,
    pub clip: Option<(f64, f64)>,
    pub quote: String,
    pub hook: String,
    pub slides: Vec<String>,
}

impl Origin {
    pub fn of(fields: &serde_json::Map<String, serde_json::Value>) -> Option<Origin> {
        use crate::frontmatter::{list, text};
        let source = text(fields, "repurposed_from").filter(|s| !s.is_empty())?;
        let number = |k| text(fields, k).and_then(|s| s.parse::<f64>().ok());
        Some(Origin {
            source,
            clip: number("clip_start").zip(number("clip_end")),
            quote: text(fields, "clip_quote").unwrap_or_default(),
            hook: text(fields, "hook").unwrap_or_default(),
            slides: list(fields, "slides"),
        })
    }

    pub fn source_name(&self) -> String {
        Path::new(&self.source)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.source.clone())
    }
}

#[cfg(test)]
mod tests;

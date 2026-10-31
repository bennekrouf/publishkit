//! `repurpose`: one recording and its transcript queued as drafts for every network and the blog.

use super::*;
use crate::repurpose::{self as rp, BlogKind, Request, Transcript};

pub(super) async fn repurpose(args: &Value) -> Result<Value, String> {
    let project = project_path(args)?;
    let cfg = ProjectConfig::load(&project);
    let lang = language(args, &cfg)?;
    let media = PathBuf::from(text(args, "media")?);
    if !media.is_absolute() || !media.is_file() {
        return Err(format!(
            "{} is not a file (an absolute path)",
            media.display()
        ));
    }
    if !rp::is_media(&media) {
        return Err(format!(
            "{} is not a video or audio file ({})",
            media.display(),
            rp::MEDIA_EXTENSIONS.join(", ")
        ));
    }
    let given = args["transcript"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let file = args["transcript_file"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let transcript = match (given, file) {
        (Some(t), _) => Transcript::parse(t),
        (None, Some(path)) if path.is_absolute() && path.is_file() => {
            Transcript::read(&path).map_err(|e| format!("{e:#}"))?
        }
        (None, Some(path)) => {
            return Err(format!("{} is not a file (an absolute path)", path.display()))
        }
        (None, None) => match rp::sidecar(&media) {
            Some(path) => Transcript::read(&path).map_err(|e| format!("{e:#}"))?,
            None => {
                return Err(format!(
                    "repurpose needs the recording's transcript: pass transcript (its text) or \
                     transcript_file, or put {} beside it. PublishKit doesn't transcribe recordings",
                    media.with_extension("srt").display()
                ))
            }
        },
    };
    if transcript.is_empty() {
        return Err("the transcript is empty".into());
    }
    let clips = match args["clips"].as_u64() {
        None => rp::DEFAULT_CLIPS,
        Some(n) if (rp::MIN_CLIPS as u64..=rp::MAX_CLIPS as u64).contains(&n) => n as usize,
        Some(n) => {
            return Err(format!(
                "clips is {} to {}, not {n}",
                rp::MIN_CLIPS,
                rp::MAX_CLIPS
            ))
        }
    };
    let blog = match args["blog"].as_str() {
        None => {
            if cfg.blog {
                BlogKind::Draft
            } else {
                BlogKind::None
            }
        }
        Some(k) => {
            BlogKind::from_key(k).ok_or(format!("blog is draft, outline or none, not {k}"))?
        }
    };
    if blog != BlogKind::None && !cfg.blog {
        return Err("this folder keeps social posts only: pass blog: none".into());
    }
    let site_networks = cfg.networks();
    let networks = match args["networks"].as_array() {
        None => site_networks,
        Some(items) => {
            let mut picked = Vec::new();
            for item in items {
                let key = item.as_str().unwrap_or("");
                let channel = Channel::from_key(key).ok_or(format!(
                    "networks are linkedin, x, instagram or tiktok, not {key}"
                ))?;
                if !picked.contains(&channel) {
                    picked.push(channel);
                }
            }
            picked
        }
    };
    let timed = transcript.timed();
    let request = Request {
        media,
        transcript,
        lang: lang.clone(),
        title: args["title"].as_str().unwrap_or("").to_string(),
        clips,
        blog,
        networks,
    };
    let (app, key) = model()?;
    let (plan, mut notes) = rp::generate(&request, &cfg, &app.provider, &key)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let saved = rp::save(&project, &cfg, &request, &plan).map_err(|e| format!("{e:#}"))?;
    if !timed && !plan.clips.is_empty() {
        notes.push(
            "the transcript has no timestamps: each clip gives the words it starts on, not its times"
                .into(),
        );
    }
    let clip_of = |s: &rp::Saved| {
        let c = plan.clips.get(s.clip?)?;
        Some(match (c.start, c.end) {
            (Some(a), Some(b)) => json!({ "start": a, "end": b }),
            _ => json!({ "quote": c.quote }),
        })
    };
    let drafts: Vec<Value> = saved
        .iter()
        .map(|s| {
            json!({
                "kind": s.kind(),
                "filename": s.filename(),
                "topic": s.topic,
                "clip": clip_of(s),
            })
        })
        .collect();
    let mut v = json!({
        "title": plan.title,
        "lang": lang,
        "drafts": drafts,
        "slides": (!plan.slides.is_empty()).then_some(&plan.slides),
    });
    if !notes.is_empty() {
        v["note"] = json!(notes.join("; "));
    }
    Ok(v)
}

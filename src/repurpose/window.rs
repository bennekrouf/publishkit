//! The Repurpose window: pick a recording and its transcript, and its drafts are queued.

use super::*;
use crate::state::{load_posts, AppConfig, ProjectPosts};
use dioxus::prelude::*;

/// What a run left: the drafts and the notes on them, or why it failed.
#[derive(Clone, PartialEq)]
enum Outcome {
    Done {
        saved: Vec<Saved>,
        notes: Vec<String>,
    },
    Failed(String),
}

/// A recording another Mayorana app sent here (PolishCast's "Send to PublishKit"): a note in
/// the folder they share, beside the licence, read once and removed.
#[derive(Clone, Debug, PartialEq)]
pub struct Handoff {
    pub media: PathBuf,
    pub transcript: Option<PathBuf>,
    pub title: String,
}

/// Older than this, a note was left by a run that never got here, and isn't acted on.
const HANDOFF_MAX_AGE_SECS: u64 = 3600;

fn handoff_path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("mayorana")
            .join("handoff")
            .join("publishkit-repurpose.json"),
    )
}

/// The recording sent here since the last look, if any.
pub fn take_handoff() -> Option<Handoff> {
    let path = handoff_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    handoff_from(&text, std::time::SystemTime::now())
}

pub(super) fn handoff_from(text: &str, now: std::time::SystemTime) -> Option<Handoff> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let now = now.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    if now.saturating_sub(v["at"].as_u64().unwrap_or(0)) > HANDOFF_MAX_AGE_SECS {
        return None;
    }
    let media = PathBuf::from(v["media"].as_str()?);
    if !media.is_absolute() || !is_media(&media) {
        return None;
    }
    let transcript = v["transcript"]
        .as_str()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    Some(Handoff {
        media,
        transcript,
        title: v["title"].as_str().unwrap_or("").to_string(),
    })
}

#[component]
pub fn RepurposeModal(
    show: Signal<bool>,
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    notice: Signal<String>,
    /// What another app sent, to start from.
    handoff: Option<Handoff>,
) -> Element {
    let sent = handoff.clone();
    let project = config.read().project_path.clone().unwrap_or_default();
    let site = ProjectConfig::load(&project);
    let languages = site.languages();
    let mut media = use_signal({
        let sent = sent.clone();
        move || sent.map(|h| h.media)
    });
    let mut transcript_file = use_signal({
        let sent = sent.clone();
        move || sent.and_then(|h| h.transcript.or_else(|| sidecar(&h.media)))
    });
    let title = sent.map(|h| h.title).unwrap_or_default();
    let mut pasted = use_signal(String::new);
    let mut lang = use_signal({
        let first = languages.first().cloned().unwrap_or_default();
        move || first
    });
    let mut clips = use_signal(|| DEFAULT_CLIPS);
    let has_blog = site.blog;
    let mut blog = use_signal(move || {
        if has_blog {
            BlogKind::Draft
        } else {
            BlogKind::None
        }
    });
    let mut running = use_signal(|| false);
    let mut outcome = use_signal(|| None::<Outcome>);
    let networks = site.networks();
    let ready = media.read().is_some()
        && (transcript_file.read().is_some() || !pasted.read().trim().is_empty())
        && !running();
    let media_name = media
        .read()
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let transcript_name = transcript_file
        .read()
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let close = move |_| {
        if !running() {
            show.set(false);
        }
    };

    rsx! {
        div { class: "modal-backdrop", onclick: close }
        div { class: "modal repurpose-modal",
            div { class: "modal-head",
                h2 { "Repurpose a recording" }
                button { class: "modal-close", title: "Close", onclick: close, "×" }
            }
            div { class: "modal-body",
                p { class: "settings-hint",
                    "A finished PolishCast video or TrackCut recording, and its transcript, become short clips to cut, \
                     carousel slides, a post for each network and a blog draft. Everything is queued as a draft: edit, \
                     reorder or delete what you don't want before you post it."
                }
                div { class: "settings-field",
                    label { "Recording" }
                    div { class: "repurpose-pick",
                        span { class: "repurpose-file", { media_name.unwrap_or_else(|| "None chosen".into()) } }
                        button {
                            class: "btn-small",
                            disabled: running(),
                            onclick: move |_| {
                                spawn(async move {
                                    let Some(file) = rfd::AsyncFileDialog::new()
                                        .set_title("Choose a video or audio recording")
                                        .add_filter("Video and audio", &MEDIA_EXTENSIONS)
                                        .pick_file()
                                        .await
                                    else {
                                        return;
                                    };
                                    let path = file.path().to_path_buf();
                                    // Its subtitles, when they are beside it by the same name.
                                    if transcript_file.peek().is_none() {
                                        transcript_file.set(sidecar(&path));
                                    }
                                    media.set(Some(path));
                                });
                            },
                            "Choose…"
                        }
                    }
                }
                div { class: "settings-field",
                    label { "Transcript" }
                    div { class: "repurpose-pick",
                        span { class: "repurpose-file",
                            { transcript_name.clone().unwrap_or_else(|| "None chosen: choose a file or paste it below".into()) }
                        }
                        if transcript_name.is_some() {
                            button {
                                class: "btn-small",
                                disabled: running(),
                                onclick: move |_| transcript_file.set(None),
                                "Remove"
                            }
                        }
                        button {
                            class: "btn-small",
                            disabled: running(),
                            onclick: move |_| {
                                spawn(async move {
                                    let Some(file) = rfd::AsyncFileDialog::new()
                                        .set_title("Choose the transcript")
                                        .add_filter("Subtitles or text", &TRANSCRIPT_EXTENSIONS)
                                        .pick_file()
                                        .await
                                    else {
                                        return;
                                    };
                                    transcript_file.set(Some(file.path().to_path_buf()));
                                });
                            },
                            "Choose…"
                        }
                    }
                    if transcript_name.is_none() {
                        textarea {
                            class: "repurpose-paste",
                            rows: "5",
                            placeholder: "Or paste the transcript here",
                            value: "{pasted}",
                            disabled: running(),
                            oninput: move |e| pasted.set(e.value()),
                        }
                    }
                    p { class: "settings-hint",
                        "Subtitles (.srt or .vtt) give each clip its start and end; plain text only says which passage to cut."
                    }
                }
                div { class: "repurpose-row",
                    if languages.len() > 1 {
                        div { class: "settings-field",
                            label { "Language" }
                            select {
                                disabled: running(),
                                onchange: move |e: Event<FormData>| lang.set(e.value()),
                                for l in languages.iter() {
                                    option { key: "{l}", value: "{l}", selected: *l == lang(), "{l.to_uppercase()}" }
                                }
                            }
                        }
                    }
                    if networks.iter().any(|c| matches!(c, Channel::TikTok | Channel::Instagram)) {
                        div { class: "settings-field",
                            label { "Clips" }
                            select {
                                disabled: running(),
                                onchange: move |e: Event<FormData>| {
                                    if let Ok(n) = e.value().parse() {
                                        clips.set(n);
                                    }
                                },
                                for n in MIN_CLIPS..=MAX_CLIPS {
                                    option { key: "{n}", value: "{n}", selected: n == clips(), "{n}" }
                                }
                            }
                        }
                    }
                    if site.blog {
                        div { class: "settings-field",
                            label { "Blog" }
                            select {
                                disabled: running(),
                                onchange: move |e: Event<FormData>| {
                                    if let Some(k) = BlogKind::from_key(&e.value()) {
                                        blog.set(k);
                                    }
                                },
                                for k in BlogKind::ALL {
                                    option { key: "{k.key()}", value: "{k.key()}", selected: k == blog(), "{k.label()}" }
                                }
                            }
                        }
                    }
                }
                match outcome.read().clone() {
                    Some(Outcome::Done { saved, notes }) => rsx! {
                        div { class: "repurpose-result",
                            p { class: "settings-saved", "{saved.len()} drafts queued" }
                            ul {
                                for s in saved.iter() {
                                    li { key: "{s.path.display()}",
                                        span { class: "repurpose-kind", { s.channel.map_or("Blog", Channel::label) } }
                                        " {s.topic}"
                                    }
                                }
                            }
                            for n in notes.iter() {
                                p { class: "settings-hint", "{n}" }
                            }
                        }
                    },
                    Some(Outcome::Failed(why)) => rsx! { p { class: "repurpose-error", "{why}" } },
                    None => rsx! {},
                }
            }
            div { class: "modal-footer",
                if matches!(*outcome.read(), Some(Outcome::Done { .. })) {
                    button { class: "btn-primary", onclick: move |_| show.set(false), "Done" }
                } else {
                    button {
                        class: "btn-primary",
                        disabled: !ready,
                        onclick: {
                            let project = project.clone();
                            let networks = networks.clone();
                            let title = title.clone();
                            move |_| {
                                let project = project.clone();
                                let networks = networks.clone();
                                let title = title.clone();
                                let Some(media) = media.read().clone() else {
                                    return;
                                };
                                let transcript = transcript_file.read().clone();
                                let text = pasted.read().clone();
                                let language = lang.read().clone();
                                let (count, kind) = (clips(), blog());
                                running.set(true);
                                outcome.set(None);
                                spawn(async move {
                                    let result = run(&project, media, transcript, text, language, title, count, kind, networks).await;
                                    running.set(false);
                                    match result {
                                        Ok((saved, notes)) => {
                                            notice.set(format!("Repurposed: {} drafts queued.", saved.len()));
                                            posts.set(load_posts(&project));
                                            outcome.set(Some(Outcome::Done { saved, notes }));
                                        }
                                        Err(why) => outcome.set(Some(Outcome::Failed(why))),
                                    }
                                });
                            }
                        },
                        if running() { "Writing the drafts…" } else { "Repurpose" }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    project: &str,
    media: PathBuf,
    transcript_file: Option<PathBuf>,
    pasted: String,
    lang: String,
    title: String,
    clips: usize,
    blog: BlogKind,
    networks: Vec<Channel>,
) -> std::result::Result<(Vec<Saved>, Vec<String>), String> {
    let app = AppConfig::load();
    let key = app.resolved_api_key();
    if key.is_empty() {
        return Err(app.missing_key().to_string());
    }
    let transcript = match transcript_file {
        Some(path) => Transcript::read(&path).map_err(|e| format!("{e:#}"))?,
        None => Transcript::parse(&pasted),
    };
    let cfg = ProjectConfig::load(project);
    let request = Request {
        media,
        transcript,
        lang,
        title,
        clips,
        blog,
        networks,
    };
    let (plan, notes) = generate(&request, &cfg, &app.provider, &key)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let project = project.to_string();
    let saved = tokio::task::spawn_blocking(move || save(&project, &cfg, &request, &plan))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    Ok((saved, notes))
}

use super::*;
use crate::state::load_posts;

const SRT: &str = "1\n00:00:01,000 --> 00:00:04,500\nHello <i>there</i>.\n\n\
                   2\n00:01:02,000 --> 00:01:10,250\nSecond line\nwraps here.\n\n\
                   3\n00:03:20,000 --> 00:03:30,000\nThe end.\n";

fn site() -> (PathBuf, String) {
    // A folder of its own per test: tests run in parallel.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("publishkit-repurpose-{}-{n}", std::process::id()));
    std::fs::create_dir_all(root.join("content/en/queue")).unwrap();
    let project = root.to_string_lossy().into_owned();
    (root, project)
}

fn request(transcript: Transcript, networks: Vec<Channel>, blog: BlogKind) -> Request {
    Request {
        media: PathBuf::from("/rec/talk.mp4"),
        transcript,
        lang: "en".into(),
        title: String::new(),
        clips: 3,
        blog,
        networks,
    }
}

fn clip(start: f64, end: f64, hook: &str) -> Clip {
    Clip {
        start: Some(start),
        end: Some(end),
        quote: String::new(),
        hook: hook.into(),
        caption: format!("{hook} #a #b"),
    }
}

#[test]
fn subtitles_are_read_as_timed_cues() {
    let t = Transcript::parse(SRT);
    assert!(t.timed());
    assert_eq!(t.cues.len(), 3);
    assert_eq!(t.cues[0].text, "Hello there.");
    assert_eq!(t.cues[1].start, 62.0);
    assert_eq!(t.cues[1].end, 70.25);
    assert_eq!(t.cues[1].text, "Second line wraps here.");
    assert_eq!(t.end(), Some(210.0));

    let vtt = "WEBVTT\n\n00:05.000 --> 00:09.000 align:start\nShort stamps.\n";
    let t = Transcript::parse(vtt);
    assert_eq!(
        t.cues,
        vec![Cue {
            start: 5.0,
            end: 9.0,
            text: "Short stamps.".into()
        }]
    );

    let plain = Transcript::parse("\u{feff}Just words, no times.");
    assert!(!plain.timed() && !plain.is_empty());
    assert_eq!(plain.plain, "Just words, no times.");
    assert!(Transcript::parse("  \n").is_empty());
}

#[test]
fn a_timed_transcript_reaches_the_model_with_its_clock() {
    let (text, cut) = Transcript::parse(SRT).for_prompt(10_000);
    assert!(!cut);
    assert!(
        text.starts_with("[0:01] Hello there.\n[1:02] Second"),
        "{text}"
    );
    let (short, cut) = Transcript::parse(SRT).for_prompt(10);
    assert!(cut && short.chars().count() == 10);
    assert_eq!(clock(3725.0), "1:02:05");
}

#[test]
fn the_reply_is_found_inside_fences_and_chatter() {
    let raw = "Here you go:\n```json\n{\"title\": \"T\", \"x\": \"Post\", \"clips\": [{\"start\": 1, \"end\": 20, \"hook\": \"H\", \"caption\": \"C\"}]}\n```";
    let plan = parse_plan(raw).unwrap();
    assert_eq!(plan.title, "T");
    assert_eq!(plan.clips[0].end, Some(20.0));
    assert!(parse_plan("no json").is_err());
}

#[test]
fn clips_are_kept_inside_the_recording_and_apart() {
    let req = request(
        Transcript::parse(SRT),
        vec![Channel::TikTok],
        BlogKind::None,
    );
    let plan = Plan {
        clips: vec![
            clip(60.0, 90.0, "first"),
            clip(62.0, 88.0, "same moment"),
            clip(10.0, 12.0, "too short"),
            clip(190.0, 400.0, "past the end"),
            clip(100.0, 140.0, "third"),
            clip(150.0, 170.0, "one too many"),
        ],
        ..Plan::default()
    };
    let (plan, _) = tidy(plan, &req);
    let kept: Vec<_> = plan
        .clips
        .iter()
        .map(|c| (c.hook.as_str(), c.start, c.end))
        .collect();
    assert_eq!(
        kept,
        vec![
            ("first", Some(60.0), Some(90.0)),
            ("past the end", Some(190.0), Some(210.0)),
            ("third", Some(100.0), Some(140.0)),
        ]
    );
}

#[test]
fn an_untimed_transcript_gives_clips_a_passage_not_times() {
    let req = request(
        Transcript::parse("plain words"),
        vec![Channel::TikTok],
        BlogKind::None,
    );
    let mut with_quote = clip(1.0, 30.0, "hook");
    with_quote.quote = "plain words".into();
    let plan = Plan {
        clips: vec![with_quote, clip(1.0, 30.0, "no quote")],
        ..Plan::default()
    };
    let (plan, notes) = tidy(plan, &req);
    assert_eq!(plan.clips.len(), 1);
    assert_eq!(plan.clips[0].start, None);
    assert!(
        notes.iter().any(|n| n.contains("only 1 usable clip")),
        "{notes:?}"
    );
}

#[test]
fn texts_fit_each_network() {
    let req = request(
        Transcript::parse(SRT),
        Channel::ALL.to_vec(),
        BlogKind::None,
    );
    let long_x = format!("First sentence here. {}", "word ".repeat(80));
    let tags = (1..=8)
        .map(|i| format!("#t{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let plan = Plan {
        x: long_x,
        instagram: format!("Caption line.\n\n{tags}"),
        ..Plan::default()
    };
    let (plan, notes) = tidy(plan, &req);
    assert!(plan.x.chars().count() <= Channel::X.limit(), "{}", plan.x);
    assert_eq!(crate::state::hashtags(&plan.instagram), 5);
    assert_eq!(plan.instagram, "Caption line.\n\n#t1 #t2 #t3 #t4 #t5");
    assert_eq!(
        notes.iter().filter(|n| !n.contains("clip")).count(),
        2,
        "{notes:?}"
    );
}

#[test]
fn text_is_cut_at_a_sentence_or_a_word() {
    // A sentence ending before half the room is too little to keep alone.
    assert_eq!(fit("One. Two three four.", 12), "One. Two…");
    assert_eq!(fit("Short.", 10), "Short.");
    let cut = fit("alpha beta gamma delta", 12);
    assert_eq!(cut, "alpha beta…");
    assert!(fit("A long first sentence. Then more", 26).ends_with("sentence."));
}

#[test]
fn a_transcript_beside_the_recording_is_found() {
    let (root, _) = site();
    let media = root.join("talk.mp4");
    std::fs::write(&media, b"").unwrap();
    assert_eq!(sidecar(&media), None);
    std::fs::write(root.join("talk.txt"), "words").unwrap();
    std::fs::write(root.join("talk.srt"), SRT).unwrap();
    assert_eq!(sidecar(&media), Some(root.join("talk.srt")));
    assert!(is_media(&media) && !is_media(&root.join("talk.srt")));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_plan_is_queued_as_drafts_that_say_where_they_came_from() {
    let (root, project) = site();
    let cfg = ProjectConfig::load(&project);
    let req = request(Transcript::parse(SRT), cfg.networks(), BlogKind::Outline);
    let plan = Plan {
        title: "A talk".into(),
        summary: "About things.".into(),
        clips: vec![clip(60.0, 90.0, "first"), clip(100.0, 140.0, "second")],
        slides: vec!["Hook".into(), "Idea".into(), "Follow".into()],
        linkedin: "LinkedIn text".into(),
        x: "X text".into(),
        instagram: "Caption".into(),
        blog: Some(BlogDraft {
            title: "Blog title".into(),
            summary: "Sum".into(),
            body: "## One\n\n- point".into(),
        }),
    };
    let saved = save(&project, &cfg, &req, &plan).unwrap();
    let kinds: Vec<_> = saved.iter().map(Saved::kind).collect();
    assert_eq!(
        kinds,
        ["tiktok", "tiktok", "instagram", "linkedin", "x", "blog"]
    );
    assert_eq!(saved[1].clip, Some(1));
    assert_eq!(saved[0].topic, "Clip 1 · 1:00–1:30 · first");

    let posts = load_posts(&project);
    let tiktok = &posts.social(Channel::TikTok, "en").queue;
    let first = tiktok
        .iter()
        .find(|p| p.topic.starts_with("Clip 1"))
        .unwrap();
    let origin = first.origin.clone().unwrap();
    assert_eq!(origin.source, "/rec/talk.mp4");
    assert_eq!(origin.clip, Some((60.0, 90.0)));
    assert_eq!(origin.hook, "first");
    assert_eq!(first.body.trim(), "first #a #b");
    let carousel = &posts.social(Channel::Instagram, "en").queue[0];
    assert_eq!(
        carousel.origin.as_ref().unwrap().slides,
        ["Hook", "Idea", "Follow"]
    );

    // An outline waits on the board as an idea.
    let blog = &posts.blog("en").queue[0];
    assert_eq!(blog.title, "Blog title");
    assert_eq!(blog.stage, Stage::Idea);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_recording_sent_by_polishcast_is_picked_up_while_fresh() {
    use super::window::handoff_from;
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    let note =
        r#"{"media": "/m/talk.mp4", "transcript": "/m/talk.srt", "title": "Demo", "at": 999000}"#;
    assert_eq!(
        handoff_from(note, at),
        Some(Handoff {
            media: "/m/talk.mp4".into(),
            transcript: Some("/m/talk.srt".into()),
            title: "Demo".into()
        })
    );
    // Left for over an hour, or not a recording: ignored.
    let later = at + std::time::Duration::from_secs(3 * 3600);
    assert_eq!(handoff_from(note, later), None);
    assert_eq!(
        handoff_from(r#"{"media": "/m/notes.txt", "at": 999000}"#, at),
        None
    );
}

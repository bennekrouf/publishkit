use super::*;

#[test]
fn the_preview_finds_a_sites_images_and_leaves_others_alone() {
    let root = std::env::temp_dir().join(format!("publishkit-preview-{}", std::process::id()));
    std::fs::create_dir_all(root.join("public/images/blog")).unwrap();
    std::fs::create_dir_all(root.join("posts/one")).unwrap();
    std::fs::write(root.join("public/images/blog/a.png"), "png").unwrap();
    std::fs::write(root.join("posts/one/b.png"), "png").unwrap();
    let project = root.to_string_lossy().into_owned();
    let post = root.join("posts/one/index.md");
    let data = local_image(
        "/images/blog/a.png",
        &post,
        &project,
        "public/images/blog",
        "/images/blog",
    )
    .unwrap();
    assert_eq!(data, "data:image/png;base64,cG5n");
    assert!(local_image("b.png?v=2", &post, &project, "images", "/images").is_some());
    assert_eq!(
        local_image("https://x.org/a.png", &post, &project, "images", "/images"),
        None
    );
    assert_eq!(
        local_image("/missing.png", &post, &project, "images", "/images"),
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn social(channel: Channel, body: &str, media: &[&str]) -> SocialPost {
    SocialPost {
        channel,
        path: std::path::PathBuf::from("/q/p.md"),
        filename: "p.md".into(),
        topic: "P".into(),
        lang: "en".into(),
        status: "draft".into(),
        date: String::new(),
        when: None,
        publish_at: None,
        body: body.into(),
        media: media.iter().map(|m| m.to_string()).collect(),
        origin: None,
        url: None,
    }
}

#[test]
fn an_instagram_caption_is_told_what_it_lacks() {
    let bare = notes(&social(Channel::Instagram, "New https://x.org", &[]));
    assert_eq!(bare.len(), 2, "{bare:?}");
    assert!(bare[0].contains("needs a photo or a video"));
    assert!(bare[1].contains("can't be clicked"));

    let tags = (0..6)
        .map(|i| format!("#t{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let tagged = notes(&social(
        Channel::Instagram,
        &format!("Hi # {tags}"),
        &["a.png"],
    ));
    assert_eq!(tagged.len(), 1, "{tagged:?}");
    assert!(
        tagged[0].starts_with("Instagram takes at most 5 hashtags in a post or reel; this has 6.")
    );

    // LinkedIn and X need no media, and their links work.
    assert!(notes(&social(Channel::X, "New https://x.org", &[])).is_empty());
}

#[test]
fn a_folder_of_social_posts_only_shows_no_blog_and_hides_what_was_unticked() {
    let site = ProjectConfig {
        blog: false,
        hidden_networks: vec!["x".into()],
        ..ProjectConfig::default()
    };
    assert_eq!(
        ContentMode::shown(&site),
        [
            ContentMode::Social(Channel::LinkedIn),
            ContentMode::Social(Channel::Instagram),
            ContentMode::Social(Channel::TikTok),
            ContentMode::Calendar,
            ContentMode::Library,
        ]
    );
    assert_eq!(
        ContentMode::shown(&ProjectConfig::default()),
        ContentMode::ALL
    );
}

#[test]
fn an_image_is_warned_about_only_when_its_network_would_crop_or_soften_it() {
    // Instagram: 4:5 to 1.91:1 at 1080 wide is fine; a tall phone photo is cropped.
    assert_eq!(image_note(Channel::Instagram, "a.jpg", 1080, 1350), None);
    assert_eq!(image_note(Channel::Instagram, "a.jpg", 1080, 566), None);
    let tall = image_note(Channel::Instagram, "a.jpg", 1080, 1920).unwrap();
    assert!(
        tall.starts_with("a.jpg is 1080×1920: Instagram crops"),
        "{tall}"
    );
    assert!(image_note(Channel::Instagram, "a.jpg", 640, 800)
        .unwrap()
        .contains("soft"));
    // TikTok wants it vertical.
    assert_eq!(image_note(Channel::TikTok, "a.jpg", 1080, 1920), None);
    assert!(image_note(Channel::TikTok, "a.jpg", 1920, 1080)
        .unwrap()
        .contains("9:16"));
    // LinkedIn and X take most sizes: only a small one is mentioned.
    assert_eq!(image_note(Channel::LinkedIn, "a.jpg", 1200, 627), None);
    assert_eq!(image_note(Channel::X, "a.jpg", 1600, 900), None);
    assert!(image_note(Channel::X, "a.jpg", 600, 400)
        .unwrap()
        .contains("small for X"));
}

#[test]
fn a_real_image_in_a_post_is_measured() {
    let dir = std::env::temp_dir().join(format!("publishkit-size-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A 1×1 PNG: far too small, and square, for TikTok.
    let png = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0,
        0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89,
    ];
    std::fs::write(dir.join("a.png"), png).unwrap();
    let mut post = social(Channel::TikTok, "Hi", &["a.png"]);
    post.path = dir.join("p.md");
    let found = notes(&post);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("a.png is 1×1: TikTok fills the screen"),
        "{found:?}"
    );
}

#[test]
fn a_social_draft_is_due_once_its_planned_time_has_passed() {
    let planned = |at: Option<&str>| SocialPost {
        publish_at: at.map(String::from),
        ..social(Channel::TikTok, "Hi", &["clip.mp4"])
    };
    let mut posts = ProjectPosts::empty();
    posts.languages = vec!["en".into()];
    let shelf = crate::state::Shelf {
        queue: vec![
            planned(Some("2026-11-03T09:00")),
            planned(Some("2026-11-03T18:00")),
            planned(Some("2026-11-02")),
            planned(None),
        ],
        published: Vec::new(),
    };
    posts
        .social
        .entry(Channel::TikTok)
        .or_default()
        .insert("en".into(), shelf);
    let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 3)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let due: Vec<Option<String>> = social_due(&posts, now)
        .into_iter()
        .map(|p| p.publish_at)
        .collect();
    assert_eq!(
        due,
        [
            Some("2026-11-03T09:00".to_string()),
            Some("2026-11-02".to_string())
        ]
    );
}

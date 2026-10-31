use super::*;
use crate::state::{load_posts, Channel, Preset};

fn site(files: &[(&str, &str)]) -> (PathBuf, String) {
    // A folder of its own per test: tests run in parallel, and a clock can repeat.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("publishkit-api-{}-{n}", std::process::id()));
    for (path, content) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let project = root.to_string_lossy().into_owned();
    (root, project)
}

#[test]
fn a_flagged_draft_is_published_in_place_and_keeps_its_other_fields() {
    let (root, project) = site(&[
        ("hugo.toml", ""),
        ("content/posts/a.md", "+++\ntitle = \"A\"\n# mine\ndate = 2026-01-01T09:00:00Z\ndraft = true\n[params]\nx = 1\n+++\nBody"),
    ]);
    let cfg = ProjectConfig::load(&project);
    assert_eq!(cfg.preset, Some(Preset::Hugo));
    let done = publish_post(&project, "en", "a.md", &cfg, None).unwrap();
    assert_eq!(done.path, root.join("content/posts/a.md"));
    assert_eq!(done.command, None);
    let content = std::fs::read_to_string(&done.path).unwrap();
    assert!(
        content.contains("draft = false\n[params]\nx = 1\n+++\nBody"),
        "{content}"
    );
    assert!(
        content.contains("# mine") && !content.contains("status"),
        "{content}"
    );
    assert!(
        content.contains(&format!(
            "date = \"{}T",
            chrono::Local::now().format("%Y-%m-%d")
        )),
        "{content}"
    );
    assert!(
        delete_queued(&project, "en", "a.md", &cfg).is_err(),
        "a published post is not a draft"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_jekyll_draft_moves_into_posts_with_the_date_in_its_name() {
    let (root, project) = site(&[
        ("_config.yml", ""),
        ("_posts/2020-01-01-old.md", "---\ntitle: Old\n---\n"),
    ]);
    let cfg = ProjectConfig::load(&project);
    let path = save_to_queue(&project, "Hello, World", "Sum", "en", "Body", &cfg).unwrap();
    assert!(
        Path::new(&path).ends_with("_drafts/hello-world.md"),
        "{path}"
    );
    let draft = std::fs::read_to_string(&path).unwrap();
    assert!(
        !draft.contains("# Hello") && draft.contains("status: \"draft\""),
        "{draft}"
    );
    assert_eq!(load_posts(&project).blog("en").queue.len(), 1);

    let done = publish_post(&project, "en", "hello-world.md", &cfg, None).unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d");
    assert_eq!(
        done.path,
        root.join(format!("_posts/{today}-hello-world.md"))
    );
    assert!(!Path::new(&path).exists());
    let shelf = load_posts(&project).blog("en");
    assert_eq!((shelf.queue.len(), shelf.published.len()), (0, 2));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_failing_publish_command_is_reported_and_the_post_stays_published() {
    let (root, project) = site(&[(
        "content/fr/queue/a.md",
        "---\ntitle: \"A\"\nstatus: \"draft\"\n---\n\nBody",
    )]);
    let mut cfg = ProjectConfig::load(&project);
    assert_eq!(cfg.preset, Some(Preset::PublishKit));
    cfg.publish_command = "echo built && echo broken >&2 && exit 3".into();
    let done = publish_post(&project, "fr", "a.md", &cfg, None).unwrap();
    let run = done.command.clone().unwrap();
    assert!(!run.ok);
    assert_eq!(run.output, "broken");
    assert!(done.summary().contains("failed: broken"));
    assert!(!done.ok());
    assert_eq!(done.deploy, None);
    let published = std::fs::read_to_string(root.join("content/fr/blog/a.md")).unwrap();
    assert!(published.contains("status: \"published\""), "{published}");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_image_is_copied_where_the_site_serves_it_under_a_name_of_its_own() {
    let (root, project) = site(&[
        ("hugo.toml", ""),
        ("content/posts/a.md", "+++\n+++\n"),
        ("in/My Photo.PNG", "png"),
    ]);
    let cfg = ProjectConfig::load(&project);
    let source = root.join("in/My Photo.PNG");
    let post = root.join("content/posts/a.md");
    assert_eq!(
        add_image(&project, &cfg, &source, &post).unwrap(),
        "/images/my-photo.png"
    );
    assert_eq!(
        add_image(&project, &cfg, &source, &post).unwrap(),
        "/images/my-photo-2.png"
    );
    assert!(root.join("static/images/my-photo-2.png").is_file());
    assert!(add_image(&project, &cfg, &root.join("hugo.toml"), &post).is_err());
    let bundle = root.join("content/posts/b/index.md");
    std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
    assert_eq!(
        add_image(&project, &cfg, &source, &bundle).unwrap(),
        "my-photo.png"
    );
    assert!(root.join("content/posts/b/my-photo.png").is_file());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_scheduled_draft_is_published_once_its_time_has_come() {
    let (root, project) = site(&[
        (
            "content/en/queue/due.md",
            "---\ntitle: Due\nstatus: draft\n---\nText.",
        ),
        (
            "content/en/queue/later.md",
            "---\ntitle: Later\nstatus: draft\n---\nText.",
        ),
        (
            "content/en/queue/unfinished.md",
            "---\ntitle: Unfinished\nstatus: draft\n---\nTODO",
        ),
    ]);
    let at = |when: &'static str| {
        move |e: &mut crate::plan::Entry| e.publish_at = Some(when.to_string())
    };
    crate::plan::update(&project, "en", "due.md", at("2020-01-01T09:00")).unwrap();
    crate::plan::update(&project, "en", "later.md", at("2999-01-01T09:00")).unwrap();
    crate::plan::update(&project, "en", "gone.md", at("2020-01-01T09:00")).unwrap();
    crate::plan::update(&project, "en", "unfinished.md", at("2020-01-01T08:00")).unwrap();
    assert_eq!(
        load_posts(&project)
            .blog("en")
            .queue
            .iter()
            .filter(|p| p.publish_at.is_some())
            .count(),
        3
    );

    let done = publish_due(&project);
    assert_eq!(done.len(), 2);
    assert_eq!(done[0].0, "en/unfinished.md");
    assert!(done[0].1.as_ref().unwrap_err().to_string().contains("TODO"));
    assert!(root.join("content/en/queue/unfinished.md").is_file());
    assert_eq!(done[1].0, "en/due.md");
    assert!(root.join("content/en/blog/due.md").is_file());
    let plan = crate::plan::Plan::load(&project);
    assert_eq!(plan.drafts.keys().collect::<Vec<_>>(), ["en/later.md"]);
    assert!(publish_due(&project).is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_site_that_deploys_after_publishing_does_so_once_its_index_is_built() {
    let (root, project) = site(&[(
        "content/en/queue/a.md",
        "---\ntitle: A\nstatus: draft\n---\nText.",
    )]);
    let mut cfg = ProjectConfig::load(&project);
    cfg.publish_command = "true".into();
    cfg.deploy_command = "echo deployed > deployed.txt".into();
    cfg.deploy_after_publish = true;
    let done = publish_post(&project, "en", "a.md", &cfg, None).unwrap();
    assert!(done.ok() && done.deploy.is_some(), "{done:?}");
    assert!(root.join("deployed.txt").is_file());
    assert!(done.summary().ends_with("Deployed."));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_bundle_is_written_published_and_deleted_as_a_folder() {
    let (root, project) = site(&[
        (
            "content/en/queue/old/index.md",
            "---\ntitle: Old\nstatus: draft\n---\nText.",
        ),
        ("content/en/queue/old/cover.png", "png"),
    ]);
    let mut cfg = ProjectConfig::load(&project);
    let done = publish_post(&project, "en", "old/index.md", &cfg, None).unwrap();
    assert_eq!(done.path, root.join("content/en/blog/old/index.md"));
    assert_eq!(done.summary(), "Published old/index.md.");
    assert!(root.join("content/en/blog/old/cover.png").is_file());
    assert!(!root.join("content/en/queue/old").exists());
    assert!(std::fs::read_to_string(&done.path)
        .unwrap()
        .contains("status: \"published\""));

    cfg.bundles = true;
    let path = save_to_queue(&project, "New one", "s", "en", "Body", &cfg).unwrap();
    assert!(
        Path::new(&path).ends_with("content/en/queue/new-one/index.md"),
        "{path}"
    );
    assert_eq!(
        load_posts(&project).blog("en").queue[0].filename,
        "new-one/index.md"
    );
    delete_queued(&project, "en", "new-one/index.md", &cfg).unwrap();
    assert!(!root.join("content/en/queue/new-one").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_new_post_never_replaces_one_with_the_same_title() {
    let (root, project) = site(&[("content/en/queue/.keep", "")]);
    let cfg = ProjectConfig::load(&project);
    let first = save_to_queue(&project, "Same", "s", "en", "1", &cfg).unwrap();
    let second = save_to_queue(&project, "Same", "s", "en", "2", &cfg).unwrap();
    assert!(
        first.ends_with("same.md") && second.ends_with("same-2.md"),
        "{first} {second}"
    );
    assert!(std::fs::read_to_string(&first)
        .unwrap()
        .contains("# Same\n\n1"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_linkedin_post_marked_as_posted_keeps_the_date_and_time_it_went_out() {
    let (root, project) = site(&[(
        "content/en/linkedin/queue/ide.md",
        "---\ntopic: \"IDE\"\ndate: \"2026-10-20\"\nlang: \"en\"\nstatus: \"draft\"\n---\nBody",
    )]);
    let cfg = ProjectConfig::load(&project);
    crate::plan::plan_social(
        &project,
        Channel::LinkedIn,
        "en",
        "ide.md",
        Some("2026-10-16T09:00".into()),
    )
    .unwrap();
    assert_eq!(
        load_posts(&project).social(Channel::LinkedIn, "en").queue[0]
            .publish_at
            .as_deref(),
        Some("2026-10-16T09:00")
    );
    let at = crate::plan::When::parse("2026-10-16T09:00");
    publish_social_post(&project, &cfg, Channel::LinkedIn, "en", "ide.md", at).unwrap();
    let posted =
        std::fs::read_to_string(root.join("content/en/linkedin/published/ide.md")).unwrap();
    assert!(posted.contains("date: \"2026-10-16T09:00\""), "{posted}");
    assert!(posted.contains("status: \"published\""), "{posted}");
    let post = &load_posts(&project)
        .social(Channel::LinkedIn, "en")
        .published[0];
    assert_eq!(
        post.when.and_then(|w| w.time_label()).as_deref(),
        Some("09:00")
    );
    // Posted, it is no longer planned.
    assert_eq!(
        crate::plan::social_at(&project, Channel::LinkedIn, "en", "ide.md"),
        None
    );
}

#[test]
fn a_blog_post_published_with_a_past_date_is_dated_and_named_for_it() {
    let (root, project) = site(&[
        ("_config.yml", ""),
        ("_drafts/hello.md", "---\ntitle: \"Hello\"\n---\nBody"),
    ]);
    let cfg = ProjectConfig::load(&project);
    let at = crate::plan::When::parse("2026-10-16T09:00");
    let done = publish_post(&project, "en", "hello.md", &cfg, at).unwrap();
    let name = done
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    if cfg.date_in_filename {
        assert_eq!(name, "2026-10-16-hello.md");
    }
    let content = std::fs::read_to_string(&done.path).unwrap();
    assert!(content.contains("2026-10-16T09:00:00"), "{content}");
    let _ = root;
}

#[test]
fn set_date_rewrites_only_the_date_of_a_published_post() {
    let (root, project) = site(&[(
        "content/fr/linkedin/published/ide.md",
        "---\ntopic: \"IDE\"\ndate: \"2026-10-16\"\nlang: \"fr\"\nstatus: \"published\"\n---\nBody",
    )]);
    let path = root.join("content/fr/linkedin/published/ide.md");
    let at = crate::plan::When::parse("2026-10-16T09:00").unwrap();
    set_date(&path, true, at, Format::Yaml).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "---\ntopic: \"IDE\"\ndate: \"2026-10-16T09:00\"\nlang: \"fr\"\nstatus: \"published\"\n---\nBody"
    );
    let _ = project;
}

#[test]
fn a_linkedin_posts_media_sit_beside_it_and_go_where_it_goes() {
    let (root, project) = site(&[
        (
            "content/en/linkedin/queue/ide.md",
            "---\ntopic: \"IDE\"\ndate: \"2026-10-16\"\nlang: \"en\"\nstatus: \"draft\"\n---\nBody",
        ),
        // Another post's cover, already in the posted folder: the moved one gets a name of its own.
        ("content/en/linkedin/published/cover.png", "old"),
        ("in/Cover.png", "png"),
        ("in/two.jpg", "jpg"),
        ("in/clip.mp4", "mp4"),
    ]);
    let cfg = ProjectConfig::load(&project);
    let post = root.join("content/en/linkedin/queue/ide.md");
    let names = set_media(
        &post,
        Channel::LinkedIn,
        &[root.join("in/Cover.png"), root.join("in/two.jpg")],
    )
    .unwrap();
    assert_eq!(names, ["cover.png", "two.jpg"]);
    assert!(root.join("content/en/linkedin/queue/two.jpg").is_file());
    assert_eq!(
        load_posts(&project).social(Channel::LinkedIn, "en").queue[0].media,
        names
    );

    // Images or one video, not both; a name it has stays, the one left out goes.
    assert!(set_media(
        &post,
        Channel::LinkedIn,
        &[PathBuf::from("cover.png"), root.join("in/clip.mp4")]
    )
    .is_err());
    assert_eq!(
        set_media(&post, Channel::LinkedIn, &[PathBuf::from("two.jpg")]).unwrap(),
        ["two.jpg"]
    );
    assert!(!root.join("content/en/linkedin/queue/cover.png").exists());
    let names = set_media(
        &post,
        Channel::LinkedIn,
        &[root.join("in/Cover.png"), PathBuf::from("two.jpg")],
    )
    .unwrap();
    assert_eq!(names, ["cover.png", "two.jpg"]);

    publish_social_post(&project, &cfg, Channel::LinkedIn, "en", "ide.md", None).unwrap();
    let posted = &load_posts(&project)
        .social(Channel::LinkedIn, "en")
        .published;
    let posted = posted.iter().find(|p| p.filename == "ide.md").unwrap();
    assert_eq!(posted.media, ["cover-2.png", "two.jpg"]);
    assert_eq!(
        std::fs::read_to_string(posted.media_path("cover-2.png")).unwrap(),
        "png"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("content/en/linkedin/published/cover.png")).unwrap(),
        "old"
    );
    assert!(!root.join("content/en/linkedin/queue/two.jpg").exists());

    // Posted already: the video replaces the images, and [] takes it off again.
    let path = posted.path.clone();
    assert_eq!(
        set_media(&path, Channel::LinkedIn, &[root.join("in/clip.mp4")]).unwrap(),
        ["clip.mp4"]
    );
    assert!(!root.join("content/en/linkedin/published/two.jpg").exists());
    assert!(set_media(&path, Channel::LinkedIn, &[]).unwrap().is_empty());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(!content.contains("media"), "{content}");
}

#[test]
fn deleting_a_linkedin_draft_deletes_its_media() {
    let (root, project) = site(&[
        (
            "content/en/linkedin/queue/ide.md",
            "---\ntopic: \"IDE\"\nmedia: [\"a.gif\"]\n---\nBody",
        ),
        ("content/en/linkedin/queue/a.gif", "gif"),
        ("content/en/linkedin/queue/other.gif", "gif"),
    ]);
    let cfg = ProjectConfig::load(&project);
    delete_social_queued(&project, &cfg, Channel::LinkedIn, "en", "ide.md").unwrap();
    assert!(!root.join("content/en/linkedin/queue/a.gif").exists());
    assert!(root.join("content/en/linkedin/queue/other.gif").exists());
}

#[test]
fn a_post_takes_as_many_images_as_its_network() {
    let (root, _) = site(&[("q/p.md", "---\ntopic: \"P\"\n---\nBody"), ("x.png", "png")]);
    let post = root.join("q/p.md");
    let many = vec![root.join("x.png"); 21];
    assert!(set_media(&post, Channel::LinkedIn, &many).is_err());
    assert!(set_media(&post, Channel::LinkedIn, &[root.join("notes.txt")]).is_err());
    let five = vec![root.join("x.png"); 5];
    let err = set_media(&post, Channel::X, &five).unwrap_err().to_string();
    assert!(err.contains("X takes at most 4 images"), "{err}");
    assert_eq!(set_media(&post, Channel::X, &five[..4]).unwrap().len(), 4);
}

#[test]
fn an_x_post_is_kept_beside_linkedin_and_listed_on_its_own() {
    let (root, project) = site(&[("content/en/queue/.keep", "")]);
    let cfg = ProjectConfig::load(&project);
    let body = "Shipped: export to PDF.\n\nhttps://site.org/en/blog/export";
    let path =
        save_social_to_queue(&project, &cfg, Channel::X, &topic_of(body), "en", body).unwrap();
    assert!(
        Path::new(&path).ends_with("content/en/x/queue/shipped-export-to-pdf.md"),
        "{path}"
    );
    // Where LinkedIn's posts are kept out of the site's content, X's are too, beside them.
    let hugo = ProjectConfig {
        linkedin_dir: ".publishkit/linkedin".into(),
        ..cfg.clone()
    };
    assert_eq!(
        hugo.resolve_social_paths(&project, Channel::X, "en").0,
        root.join(".publishkit").join("x").join("en").join("queue")
    );
    let posts = load_posts(&project);
    assert!(posts.social(Channel::LinkedIn, "en").queue.is_empty());
    let queued = &posts.social(Channel::X, "en").queue[0];
    assert_eq!(queued.channel, Channel::X);
    assert_eq!(queued.topic, "Shipped: export to PDF.");
    assert_eq!(queued.body.trim(), body);

    set_body(&queued.path, "Export to PDF is out.").unwrap();
    let content = std::fs::read_to_string(&queued.path).unwrap();
    assert!(content.starts_with("---\ntopic:"), "{content}");
    assert!(
        content.ends_with("---\n\nExport to PDF is out."),
        "{content}"
    );

    crate::plan::plan_social(
        &project,
        Channel::X,
        "en",
        &queued.filename,
        Some("2026-11-02T09:00".into()),
    )
    .unwrap();
    assert_eq!(
        crate::plan::social_at(&project, Channel::LinkedIn, "en", &queued.filename),
        None
    );
    publish_social_post(&project, &cfg, Channel::X, "en", &queued.filename, None).unwrap();
    assert!(root
        .join("content/en/x/published")
        .join(&queued.filename)
        .is_file());
    assert_eq!(
        crate::plan::social_at(&project, Channel::X, "en", &queued.filename),
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_topic_is_the_first_line_cut_at_a_word() {
    assert_eq!(topic_of("\n# Hello there\nmore"), "Hello there");
    let long = "This first line is much longer than a list has room for, so it is cut short";
    assert_eq!(
        topic_of(long),
        "This first line is much longer than a list has room for, so…"
    );
    assert_eq!(topic_of("   "), "");
}

#[test]
fn addresses_in_a_post_are_found_and_the_text_kept_as_it_is() {
    let body = "New release!\nRead it at https://mayorana.ch/en/blog/x.\n(see http://a.org/b_(c)) or www.d.org\nxhttps://no https://";
    let pieces = split_links(body);
    let links: Vec<&str> = pieces.iter().filter(|(_, l)| *l).map(|(p, _)| *p).collect();
    assert_eq!(
        links,
        ["https://mayorana.ch/en/blog/x", "http://a.org/b_(c)"]
    );
    let whole: String = pieces.iter().map(|(p, _)| *p).collect();
    assert_eq!(whole, body);
    assert_eq!(split_links("no link here"), [("no link here", false)]);
    assert_eq!(split_links(""), []);
}

#[test]
fn a_written_post_has_its_link_bare_and_on_a_line_of_its_own() {
    let link = "https://mayorana.ch/en/apps/publishkit";
    // Put in by the model as asked: left alone.
    let good = format!("Big news.\n\n{link}\n\n#rust");
    assert_eq!(place_link(&good, link), good);
    // Markdown and angle brackets undone.
    assert_eq!(
        place_link(&format!("Try [PublishKit]({link}) and <{link}>"), link),
        format!("Try {link} and {link}")
    );
    // Left out: added before the closing hashtags.
    assert_eq!(
        place_link("Big news.\n\n#rust #desktop", link),
        format!("Big news.\n\n{link}\n\n#rust #desktop")
    );
    assert_eq!(
        place_link("Big news.", link),
        format!("Big news.\n\n{link}")
    );
    // No link asked for: nothing changes.
    assert_eq!(place_link("Big news.\n#rust", ""), "Big news.\n#rust");
}

#[test]
fn the_model_is_told_to_keep_the_link_as_it_is_only_when_there_is_one() {
    let link = "https://mayorana.ch/en/apps/publishkit";
    let asked = social_prompt(Channel::LinkedIn, "Launch", "fast", link);
    assert!(asked.contains(&format!("Link: {link}\n")));
    assert!(asked.contains("no markdown link"));
    assert!(asked.contains("no URL shortener"));
    let plain = social_prompt(Channel::LinkedIn, "Launch", "fast", "  ");
    assert!(!plain.contains("Link:"));
    assert!(plain.contains("Angle / key points: fast\n\nOutput ONLY"));
}

#[test]
fn an_instagram_carousel_mixes_photos_and_videos_and_tiktok_takes_35_photos() {
    let (root, _) = site(&[
        ("q/p.md", "---\ntopic: \"P\"\n---\nBody"),
        ("x.png", "png"),
        ("clip.mp4", "mp4"),
    ]);
    let post = root.join("q/p.md");
    let mixed = [root.join("x.png"), root.join("clip.mp4")];
    assert_eq!(
        set_media(&post, Channel::Instagram, &mixed).unwrap().len(),
        2
    );
    let err = set_media(&post, Channel::TikTok, &mixed)
        .unwrap_err()
        .to_string();
    assert!(err.contains("images or one video"), "{err}");
    let err = set_media(&post, Channel::Instagram, &vec![root.join("x.png"); 21])
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("Instagram takes at most 20 photos and videos"),
        "{err}"
    );
    assert_eq!(
        set_media(&post, Channel::TikTok, &vec![root.join("x.png"); 35])
            .unwrap()
            .len(),
        35
    );
}

#[test]
fn a_folder_for_social_posts_only_keeps_each_network_in_a_folder_of_its_own() {
    let (root, _) = site(&[]);
    let folder = root.join("Social");
    let folder = folder.to_string_lossy().to_string();
    ProjectConfig::start_social_only(&folder).unwrap();
    let cfg = ProjectConfig::load(&folder);
    assert!(!cfg.blog);
    assert_eq!(cfg.networks(), Channel::ALL);
    let path = save_social_to_queue(&folder, &cfg, Channel::TikTok, "Launch", "en", "Hi").unwrap();
    assert_eq!(
        std::path::Path::new(&path).parent().unwrap(),
        root.join("Social/tiktok/en/queue")
    );
    assert_eq!(
        load_posts(&folder)
            .social(Channel::TikTok, "en")
            .queue
            .len(),
        1
    );

    // Opened again, it is as it was: the settings someone changed are never written over.
    let mut changed = cfg.clone();
    changed.hidden_networks = vec!["x".into()];
    changed.save(&folder);
    ProjectConfig::start_social_only(&folder).unwrap();
    let again = ProjectConfig::load(&folder);
    assert!(!again.networks().contains(&Channel::X));
    assert_eq!(again.linkedin_dir, "linkedin");
}

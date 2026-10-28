use super::*;
use crate::state::{load_posts, Preset};

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
    let done = publish_post(&project, "en", "a.md", &cfg).unwrap();
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

    let done = publish_post(&project, "en", "hello-world.md", &cfg).unwrap();
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
    let done = publish_post(&project, "fr", "a.md", &cfg).unwrap();
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
    let done = publish_post(&project, "en", "a.md", &cfg).unwrap();
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
    let done = publish_post(&project, "en", "old/index.md", &cfg).unwrap();
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

use super::*;

#[test]
fn a_region_is_saved_and_read_back() {
    let mut cfg = AppConfig {
        provider: LlmProvider::api0("china"),
        ..AppConfig::default()
    };
    let back: AppConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
    assert_eq!(back.provider.region().map(|r| r.id), Some("china"));
    // Flag and name, as every app shows a region; the name alone on Windows.
    let shown = if cfg!(windows) {
        "China"
    } else {
        "🇨🇳 China"
    };
    assert_eq!(back.provider.label(), shown);
    assert_eq!(back.provider.model(), "china");
    // A config saved with a provider of its own key still reads, on the default region.
    cfg = serde_json::from_str(r#"{"project_path":"/site","provider":"Other","api_key":"k"}"#)
        .unwrap();
    assert_eq!(cfg.project_path.as_deref(), Some("/site"));
    assert_eq!(cfg.provider.region().map(|r| r.id), Some("europe"));
}

fn site(files: &[(&str, &str)]) -> PathBuf {
    // A folder of its own per test: tests run in parallel, and a clock can repeat.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("publishkit-test-{}-{n}", std::process::id()));
    for (path, content) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn a_hugo_site_is_found_with_its_drafts_flagged() {
    let root = site(&[
        ("hugo.toml", "baseURL = 'https://x.org'"),
        (
            "content/posts/a.md",
            "+++\ntitle = \"A\"\ndate = 2026-01-01\ndraft = true\n+++\nA",
        ),
        (
            "content/posts/b.md",
            "+++\ntitle = \"B\"\ndate = 2026-01-02\n+++\nB",
        ),
    ]);
    let project = root.to_string_lossy();
    assert_eq!(detect_preset(&project), Some(Preset::Hugo));
    let cfg = ProjectConfig::load(&project);
    assert_eq!(cfg.drafts, Drafts::Flag);
    assert_eq!(cfg.front_matter, Format::Toml);
    let posts = load_posts(&project);
    let shelf = posts.blog("en");
    assert_eq!(
        shelf
            .queue
            .iter()
            .map(|p| p.title.as_str())
            .collect::<Vec<_>>(),
        ["A"]
    );
    assert_eq!(
        shelf
            .published
            .iter()
            .map(|p| p.title.as_str())
            .collect::<Vec<_>>(),
        ["B"]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn every_language_folder_is_a_language() {
    let root = site(&[
        ("content/de/queue/a.md", "---\ntitle: A\n---\n"),
        ("content/fr/blog/b.md", "---\ntitle: B\n---\n"),
        ("content/pt-br/blog/c.md", "---\ntitle: C\n---\n"),
        ("content/images/x.md", "---\ntitle: X\n---\n"),
    ]);
    let project = root.to_string_lossy();
    let posts = load_posts(&project);
    assert_eq!(posts.languages, ["de", "fr", "pt-br"]);
    assert_eq!(posts.blog("de").queue.len(), 1);
    assert_eq!(posts.blog("pt-br").published[0].status, "published");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_config_from_before_presets_keeps_its_folders_and_index_script() {
    let root = site(&[
            ("scripts/generate-blog-data.js", ""),
            ("publishkit.yaml", "site_name: Old\nsite_url: https://old.org\nauthor: Me\ndefault_tags: []\n\
                system_prompt: Mine\nsocial_enabled: false\ncontent_paths:\n  langs:\n    fr:\n      queue: drafts/fr\n      blog: out/fr\n"),
        ]);
    let project = root.to_string_lossy();
    let cfg = ProjectConfig::load(&project);
    assert_eq!(cfg.preset, Some(Preset::PublishKit));
    assert_eq!(cfg.system_prompt, "Mine");
    assert_eq!(cfg.content_paths.unwrap().langs["fr"].queue, "drafts/fr");
    assert_eq!(cfg.publish_command, "node scripts/generate-blog-data.js");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn configured_folders_join_with_the_platform_separator() {
    let mut cfg = ProjectConfig::default();
    let mut langs = BTreeMap::new();
    langs.insert(
        "en".to_string(),
        LangPaths {
            queue: "content/en/queue".into(),
            blog: "content/en/blog/".into(),
            suffix: String::new(),
        },
    );
    cfg.content_paths = Some(ContentPaths { langs });
    let (queue, blog) = cfg.resolve_paths("/site", "en");
    assert_eq!(
        queue,
        Path::new("/site").join("content").join("en").join("queue")
    );
    assert_eq!(
        blog,
        Path::new("/site").join("content").join("en").join("blog")
    );
}

#[test]
fn a_config_that_does_not_parse_is_never_written_over() {
    let broken = "site_name: [unclosed\n";
    let root = site(&[
        ("publishkit.yaml", broken),
        ("posts/a.md", "---\ntitle: A\n---\n"),
    ]);
    let project = root.to_string_lossy();
    let _ = ProjectConfig::load(&project);
    assert_eq!(
        std::fs::read_to_string(root.join("publishkit.yaml")).unwrap(),
        broken
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn page_bundles_are_posts_and_list_pages_are_not() {
    let root = site(&[
        ("astro.config.mjs", ""),
        ("src/content/blog/_index.md", "---\ntitle: List\n---\n"),
        (
            "src/content/blog/one/index.md",
            "---\ntitle: One\ndate: 2026-01-02\n---\n",
        ),
        ("src/content/blog/one/cover.png", ""),
        (
            "src/content/blog/two/index.mdx",
            "---\ntitle: Two\ndate: 2026-01-01\ndraft: true\n---\n",
        ),
        ("src/content/blog/notes/readme.txt", ""),
    ]);
    let project = root.to_string_lossy();
    let cfg = ProjectConfig::load(&project);
    assert!(cfg.bundles);
    let shelf = load_posts(&project).blog("en");
    assert_eq!(
        shelf
            .published
            .iter()
            .map(|p| p.filename.as_str())
            .collect::<Vec<_>>(),
        ["one/index.md"]
    );
    assert_eq!(
        shelf
            .queue
            .iter()
            .map(|p| p.filename.as_str())
            .collect::<Vec<_>>(),
        ["two/index.mdx"]
    );
    assert_eq!(slug_of("one/index.md"), "one");
    let stamp = content_stamp(&project);
    assert!(stamp.iter().any(|(p, _)| p.ends_with("one/index.md")));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_open_site_comes_first_and_gone_folders_are_left_out() {
    let a = site(&[("publishkit.yaml", "site_name: Alpha\n")]);
    let b = site(&[("x", "")]);
    let (a_path, b_path) = (
        a.to_string_lossy().into_owned(),
        b.to_string_lossy().into_owned(),
    );
    let mut cfg = AppConfig {
        project_path: Some(a_path.clone()),
        ..Default::default()
    };
    assert_eq!(
        cfg.sites(),
        std::slice::from_ref(&a_path),
        "a site open before the list existed is in it"
    );
    cfg.open_project(&b_path);
    cfg.open_project("/no/such/folder");
    cfg.open_project(&b_path);
    assert_eq!(cfg.sites(), [b_path.clone(), a_path.clone()]);
    assert_eq!(site_label(&a_path), "Alpha");
    assert_eq!(
        site_label(&b_path),
        b.file_name().unwrap().to_string_lossy()
    );
    cfg.forget_project(&b_path);
    assert_eq!(cfg.project_path.as_deref(), Some(a_path.as_str()));
    std::fs::remove_dir_all(a).unwrap();
    std::fs::remove_dir_all(b).unwrap();
}

#[test]
fn languages_marked_in_file_names_are_languages() {
    let root = site(&[
        (
            "hugo.toml",
            "baseURL = 'https://x.org'\ndefaultContentLanguage = 'fr'\n",
        ),
        ("content/posts/a.md", "+++\ntitle = \"A fr\"\n+++\n"),
        ("content/posts/a.en.md", "+++\ntitle = \"A en\"\n+++\n"),
        (
            "content/posts/b.en.md",
            "+++\ntitle = \"B en\"\ndraft = true\n+++\n",
        ),
        ("content/posts/c/index.md", "+++\ntitle = \"C fr\"\n+++\n"),
        (
            "content/posts/c/index.en.md",
            "+++\ntitle = \"C en\"\n+++\n",
        ),
        ("content/posts/v1.2.md", "+++\ntitle = \"Version\"\n+++\n"),
    ]);
    let project = root.to_string_lossy();
    let cfg = ProjectConfig::load(&project);
    assert_eq!(cfg.languages(), ["en", "fr"]);
    assert_eq!(
        (cfg.language_mark("en"), cfg.language_mark("fr")),
        ("en", "")
    );
    let posts = load_posts(&project);
    let titles = |lang: &str| {
        let shelf = posts.blog(lang);
        let mut t: Vec<String> = shelf
            .queue
            .iter()
            .chain(&shelf.published)
            .map(|p| p.title.clone())
            .collect();
        t.sort();
        t
    };
    assert_eq!(titles("fr"), ["A fr", "C fr", "Version"]);
    assert_eq!(titles("en"), ["A en", "B en", "C en"]);
    assert_eq!(cfg.counterpart("a.md", "fr", "en"), "a.en.md");
    assert_eq!(cfg.counterpart("b.en.md", "en", "fr"), "b.md");
    assert_eq!(cfg.counterpart("c/index.md", "fr", "en"), "c/index.en.md");
    assert_eq!(slug_of("a.en.md"), "a");
    assert_eq!(slug_of("v1.2.md"), "v1.2");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn slugs_and_addresses() {
    assert_eq!(slug_of("2026-01-02-hello-world.md"), "hello-world");
    assert_eq!(slug_of("hello.mdx"), "hello");
    let cfg = ProjectConfig {
        site_url: "https://x.org/".into(),
        post_url: "{site_url}/{lang}/p/{slug}/".into(),
        ..Default::default()
    };
    assert_eq!(cfg.post_url("de", "a-b.md"), "https://x.org/de/p/a-b/");
    assert!(
        is_lang_code("en")
            && is_lang_code("pt-br")
            && !is_lang_code("blog")
            && !is_lang_code("img1")
    );
}

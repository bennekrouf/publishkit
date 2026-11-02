use super::*;

fn post(body: &str, media: &[&str]) -> SocialPost {
    SocialPost {
        channel: Channel::X,
        path: std::path::PathBuf::from("/nowhere/post.md"),
        filename: "post.md".into(),
        topic: "Topic".into(),
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

/// Renders `piece` with fresh signals; `editing` starts as given.
fn render(editing: Option<&str>, piece: fn(Ctx, Signal<Option<String>>) -> Element) -> String {
    #[derive(Props, Clone)]
    struct H {
        editing: Option<String>,
        piece: fn(Ctx, Signal<Option<String>>) -> Element,
    }
    // Rendered once: there is nothing to compare.
    impl PartialEq for H {
        fn eq(&self, _: &Self) -> bool {
            false
        }
    }
    #[allow(non_snake_case)]
    fn Harness(h: H) -> Element {
        let ctx = Ctx {
            config: use_signal(AppConfig::default),
            posts: use_signal(ProjectPosts::empty),
            selected: use_signal(|| None),
            media_error: use_signal(String::new),
        };
        let editing = use_signal(|| h.editing.clone());
        (h.piece)(ctx, editing)
    }
    let mut dom = VirtualDom::new_with_props(
        Harness,
        H {
            editing: editing.map(String::from),
            piece,
        },
    );
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn the_body_links_its_addresses_and_counts_its_characters() {
    let html = render(None, |ctx, editing| {
        card_body(
            &post("Read https://example.com now", &[]),
            Channel::X,
            editing,
            ctx,
        )
    });
    assert!(html.contains(r#"href="https://example.com""#), "{html}");
    assert!(html.contains("social-card-foot"), "{html}");
    assert!(!html.contains("textarea"), "{html}");
}

#[test]
fn editing_shows_the_box_with_the_text_being_changed() {
    let html = render(Some("New text"), |ctx, editing| {
        card_body(&post("Old", &[]), Channel::X, editing, ctx)
    });
    assert!(html.contains("textarea"), "{html}");
    assert!(html.contains("New text"), "{html}");
    assert!(html.contains("Save"), "{html}");
}

#[test]
fn the_actions_offer_edit_and_post_now_on_a_draft_only() {
    let draft = render(None, |_ctx, editing| {
        let s = use_signal(String::new);
        card_actions(
            &post("Hi", &[]),
            Channel::X,
            "Images or one video",
            editing,
            use_signal(|| false),
            s,
            use_signal(|| false),
        )
    });
    assert!(
        draft.contains("Edit") && draft.contains("Post now"),
        "{draft}"
    );
    let published = render(Some("x"), |_ctx, editing| {
        let mut p = post("Hi", &[]);
        p.status = "published".into();
        let s = use_signal(String::new);
        card_actions(
            &p,
            Channel::X,
            "Images or one video",
            editing,
            use_signal(|| false),
            s,
            use_signal(|| false),
        )
    });
    assert!(
        !published.contains("Edit") && !published.contains("Post now"),
        "{published}"
    );
    assert!(published.contains("Copy"), "{published}");
}

#[test]
fn missing_media_is_marked_and_each_file_can_be_taken_off() {
    let html = render(None, |ctx, _| {
        let media = use_memo(|| {
            vec![
                Shown::Missing {
                    name: "a.png".into(),
                },
                Shown::Missing {
                    name: "b.png".into(),
                },
            ]
        });
        media_grid(&post("Hi", &["a.png", "b.png"]), media, " media-ig", ctx)
    });
    assert_eq!(html.matches("missing").count(), 4, "{html}");
    assert_eq!(html.matches("li-media-remove").count(), 2, "{html}");
    assert!(html.contains("li-media media-ig"), "{html}");
}

#[test]
fn cross_posting_names_each_network() {
    let html = render(None, |_, _| {
        cross_post(
            &post("Hi", &[]),
            crate::social::Network::ALL.to_vec(),
            Default::default(),
            use_signal(|| false),
            use_signal(String::new),
        )
    });
    assert!(
        html.contains("Post on Bluesky") && html.contains("Post on Mastodon"),
        "{html}"
    );
}

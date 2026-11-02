use super::*;

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn a_hidden_network_is_unticked_and_the_others_ticked() {
    let html = render(|| {
        tabs_field(
            use_signal(|| false),
            use_signal(|| vec![Channel::X.key().to_string()]),
        )
    });
    // Blog plus the four networks, X alone left out.
    assert_eq!(
        html.matches(r#"type="checkbox""#).count(),
        1 + Channel::ALL.len(),
        "{html}"
    );
    assert_eq!(
        html.matches("checked=true").count(),
        Channel::ALL.len() - 1,
        "{html}"
    );
}

#[test]
fn every_key_and_password_field_is_masked() {
    let blog = render(|| blog_accounts(use_signal(Accounts::default)));
    assert_eq!(blog.matches(r#"type="password""#).count(), 4, "{blog}");
    assert!(blog.contains("Send copies as drafts"), "{blog}");
    let social = render(|| social_accounts(use_signal(Accounts::default)));
    assert_eq!(social.matches(r#"type="password""#).count(), 2, "{social}");
}

#[test]
fn without_a_licence_the_model_section_says_how_to_get_one() {
    let html = render(|| {
        let notes = crate::api0::REGIONS
            .iter()
            .map(|r| (r, String::new()))
            .collect();
        let first = crate::api0::REGIONS.first();
        model_section(notes, first, false, None, use_signal(LlmProvider::default))
    });
    assert_eq!(
        html.matches(r#"class="region active""#).count(),
        1,
        "{html}"
    );
    assert!(html.contains("api0 needs a licence"), "{html}");
}

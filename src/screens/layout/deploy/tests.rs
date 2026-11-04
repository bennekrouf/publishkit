use super::*;

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

fn view(saved: &str) -> DeployView {
    DeployView {
        site: "mayorana".into(),
        folder: "/Users/me/sites/mayorana".into(),
        saved: saved.into(),
    }
}

fn dialog(saved: &str, after: bool) -> Element {
    deploy_dialog(
        &view(saved),
        use_signal(String::new),
        use_signal(|| after),
        EventHandler::new(|_| {}),
        EventHandler::new(|_| {}),
        EventHandler::new(|_| {}),
        EventHandler::new(|_| {}),
    )
}

#[test]
fn it_says_which_folder_goes_online_and_the_exact_command() {
    let html = render(|| dialog("npm run deploy", false));
    assert!(html.contains("/Users/me/sites/mayorana"), "{html}");
    assert!(
        html.contains("<code class=\"deploy-value\">npm run deploy</code>"),
        "{html}"
    );
    assert!(html.contains("once, now, on this computer"), "{html}");
    // A saved command is shown, not asked for again.
    assert!(!html.contains("deploy-command"), "{html}");
}

#[test]
fn it_offers_deploy_now_cancel_and_all_settings() {
    let html = render(|| dialog("git push", false));
    for button in [">Deploy now<", ">Cancel<", "All settings\u{2026}"] {
        assert!(html.contains(button), "{button} missing: {html}");
    }
    // Not the Settings window: none of its other sections.
    assert!(
        !html.contains("Cross-posting") && !html.contains(">Settings<"),
        "{html}"
    );
}

#[test]
fn it_says_whether_it_also_runs_after_each_publish() {
    let off = render(|| dialog("git push", false));
    assert!(
        off.contains("Off: it runs only when you press Deploy."),
        "{off}"
    );
    assert_eq!(off.matches("checked=true").count(), 0, "{off}");

    let on = render(|| dialog("git push", true));
    assert!(
        on.contains("On: it also runs by itself after each post is published"),
        "{on}"
    );
    assert_eq!(on.matches("checked=true").count(), 1, "{on}");
}

#[test]
fn with_no_command_it_asks_for_one_with_examples() {
    let html = render(|| dialog("", false));
    assert!(html.contains("deploy-command"), "{html}");
    assert!(html.contains("No deploy command is set"), "{html}");
    for example in EXAMPLES {
        assert!(
            html.contains(&format!("<code>{example}</code>")),
            "{example}: {html}"
        );
    }
}

#[test]
fn deploy_now_waits_for_a_command_to_be_typed() {
    let empty = render(|| dialog("", false));
    assert!(empty.contains("disabled=true"), "{empty}");

    let typed = render(|| {
        deploy_dialog(
            &view(""),
            use_signal(|| "vercel --prod".to_string()),
            use_signal(|| false),
            EventHandler::new(|_| {}),
            EventHandler::new(|_| {}),
            EventHandler::new(|_| {}),
            EventHandler::new(|_| {}),
        )
    });
    assert!(!typed.contains("disabled=true"), "{typed}");
    assert!(typed.contains("Runs vercel --prod"), "{typed}");
}

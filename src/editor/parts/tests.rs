use super::*;
use crate::checks::{Issue, Level};

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn checks_say_nothing_to_fix_when_clean() {
    let html = render(|| {
        checks(
            vec![],
            use_signal(String::new),
            use_signal(Vec::new),
            use_signal(|| false),
            use_signal(String::new),
        )
    });
    assert!(html.contains("Nothing to fix"), "{html}");
    assert!(html.contains("Check links"), "{html}");
}

#[test]
fn checks_list_problems_warnings_and_broken_links() {
    let html = render(|| {
        let issues = vec![
            Issue {
                level: Level::Problem,
                message: "No title".into(),
            },
            Issue {
                level: Level::Warning,
                message: "Long description".into(),
            },
        ];
        checks(
            issues,
            use_signal(String::new),
            use_signal(|| vec!["https://gone.example fails".to_string()]),
            use_signal(|| true),
            use_signal(String::new),
        )
    });
    assert!(!html.contains("Nothing to fix"), "{html}");
    assert!(
        html.contains(r#"class="check problem""#) && html.contains("No title"),
        "{html}"
    );
    assert_eq!(
        html.matches(r#"class="check warning""#).count(),
        2,
        "{html}"
    );
    assert!(html.contains("Checking links…"), "{html}");
}

#[test]
fn the_schedule_offers_every_stage_and_clearing_a_date() {
    let none = render(|| schedule_row(Stage::default(), String::new(), |_| {}));
    assert_eq!(none.matches("<option").count(), Stage::ALL.len(), "{none}");
    assert!(!none.contains("✕"), "{none}");
    let dated = render(|| schedule_row(Stage::default(), "2026-11-02T09:00".into(), |_| {}));
    assert!(
        dated.contains("2026-11-02T09:00") && dated.contains("✕"),
        "{dated}"
    );
}

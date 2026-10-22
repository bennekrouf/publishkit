mod api;
mod checks;
mod crosspost;
mod editor;
mod feedback;
mod frontmatter;
mod mcp;
mod plan;
mod state;
mod screens;
mod social;
mod views;
mod telemetry;

use dioxus::prelude::*;
use screens::App;

fn main() {
    // `publishkit mcp`: the posts over stdio for an MCP client, and no window. Before the log
    // is set up: it writes to stdout, which there is the protocol.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        dotenv::dotenv().ok();
        return mcp::serve();
    }
    // `publishkit publish-due [site folder | --all]`: what is scheduled and due goes out, for
    // cron or launchd to run when the window isn't open.
    if std::env::args().nth(1).as_deref() == Some("publish-due") {
        std::process::exit(publish_due_cli());
    }
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        // Name and version, as in every mayorana app; PublishKit has no Pro.
                        .with_title(concat!("PublishKit ", env!("CARGO_PKG_VERSION")))
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 800.0))
                        .with_min_inner_size(dioxus::desktop::LogicalSize::new(900.0, 600.0)),
                )
        )
        .launch(App);
}

fn publish_due_cli() -> i32 {
    let arg = std::env::args().nth(2);
    let projects: Vec<String> = match arg.as_deref() {
        // Every site PublishKit knows.
        Some("--all") => state::AppConfig::load().sites(),
        Some(folder) => vec![folder.to_string()],
        None => state::AppConfig::load().project_path.into_iter().collect(),
    };
    let projects: Vec<String> = projects.into_iter().filter(|p| std::path::Path::new(p).is_dir()).collect();
    if projects.is_empty() {
        eprintln!("publishkit publish-due: give the site's folder or --all, or open a site in PublishKit first");
        return 2;
    }
    let several = projects.len() > 1;
    let mut failed = false;
    for project in projects {
        let site = if several { format!("{}: ", state::site_label(&project)) } else { String::new() };
        for (post, result) in api::publish_due(&project) {
            match result {
                Ok(published) => {
                    println!("{site}{post}: {}", published.summary());
                    failed |= !published.ok();
                }
                Err(e) => {
                    eprintln!("{site}{post}: couldn't publish: {e:#}");
                    failed = true;
                }
            }
        }
    }
    i32::from(failed)
}

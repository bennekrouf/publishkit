mod api;
mod api0;
mod checks;
mod crosspost;
mod editor;
mod frontmatter;
mod kit;
mod library;
mod mcp;
mod plan;
mod pro;
mod repurpose;
mod screens;
mod social;
mod state;
mod views;

use screens::App;

/// The native title bar, as in every mayorana app: name, version and licence.
pub fn window_title(status: &desktop_kit::licence::Status) -> String {
    format!(
        "PublishKit {}{}",
        env!("CARGO_PKG_VERSION"),
        desktop_kit::licence::title_badge(status)
    )
}

fn main() {
    kit::init();
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
            window_icon(dioxus::desktop::Config::new()).with_window(
                dioxus::desktop::WindowBuilder::new()
                    // Name, version and licence, as in every mayorana app.
                    .with_title(window_title(&desktop_kit::licence::current()))
                    .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 800.0))
                    .with_min_inner_size(dioxus::desktop::LogicalSize::new(900.0, 600.0)),
            ),
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
    let projects: Vec<String> = projects
        .into_iter()
        .filter(|p| std::path::Path::new(p).is_dir())
        .collect();
    if projects.is_empty() {
        eprintln!("publishkit publish-due: give the site's folder or --all, or open a site in PublishKit first");
        return 2;
    }
    let several = projects.len() > 1;
    let mut failed = false;
    for project in projects {
        let site = if several {
            format!("{}: ", state::site_label(&project))
        } else {
            String::new()
        };
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

/// The window's icon (title bar, taskbar, Alt-Tab). Windows reads it from the .exe's icon
/// resource (see build.rs) and macOS from the app bundle; Linux has to be handed it, or the
/// window shows the Dioxus logo.
fn window_icon(config: dioxus::desktop::Config) -> dioxus::desktop::Config {
    #[cfg(target_os = "linux")]
    {
        let png = include_bytes!("../assets/icon-256.png");
        if let Ok(icon) =
            dioxus::desktop::icon_from_memory::<dioxus::desktop::tao::window::Icon>(png)
        {
            return config.with_icon(icon);
        }
    }
    config
}

mod api;
mod mcp;
mod state;
mod screens;
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
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::WindowBuilder::new()
                        .with_title("Blog Manager")
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 800.0))
                        .with_min_inner_size(dioxus::desktop::LogicalSize::new(900.0, 600.0)),
                )
        )
        .launch(App);
}

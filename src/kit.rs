//! Names PublishKit to desktop-kit, the code every mayorana desktop app shares
//! (usage statistics and the licence), and lends it PublishKit's HTTP client.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use desktop_kit::http::{Method, Request, Response};

/// First thing in `main`: until then, nothing in desktop-kit records or sends.
pub fn init() {
    desktop_kit::init(desktop_kit::App {
        id: "publishkit",
        name: "PublishKit",
        version: env!("CARGO_PKG_VERSION"),
        // Blog Toolkit, the app's name before PublishKit: the folder holds the
        // install's identity and the person's answer, and a new folder would
        // forget a "turn it off".
        telemetry_folder: "blog-toolkit",
        product: crate::api0::PRODUCT,
        licence_file,
        http,
    });
}

fn http(req: Request) -> Pin<Box<dyn Future<Output = Option<Response>> + Send>> {
    Box::pin(async move {
        let client = reqwest::Client::new();
        let mut request = match req.method {
            Method::Get => client.get(&req.url),
            Method::Post => client.post(&req.url),
        }
        .header(reqwest::header::USER_AGENT, req.user_agent)
        .timeout(req.timeout);
        for (name, value) in req.headers {
            request = request.header(name, value);
        }
        if let Some(json) = req.json {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(json);
        }
        let response = request.send().await.ok()?;
        let status = response.status().as_u16();
        let body = response.text().await.ok()?;
        Some(Response { status, body })
    })
}

/// Where the app keeps api0's last answer about its licence.
fn licence_file() -> Option<PathBuf> {
    crate::state::AppConfig::config_path()
        .parent()
        .map(|dir| dir.join("licence.json"))
}

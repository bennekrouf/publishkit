//! Names PublishKit to desktop-kit, the code every mayorana desktop app shares
//! (usage statistics and the licence), and lends it PublishKit's HTTP client.

use std::future::Future;
use std::pin::Pin;

use desktop_kit::http::{Request, Response};

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
        http_get,
    });
}

fn http_get(req: Request) -> Pin<Box<dyn Future<Output = Option<Response>> + Send>> {
    Box::pin(async move {
        let response = reqwest::Client::new()
            .get(&req.url)
            .header(reqwest::header::USER_AGENT, req.user_agent)
            .timeout(req.timeout)
            .send()
            .await
            .ok()?;
        let status = response.status().as_u16();
        let body = response.text().await.ok()?;
        Some(Response { status, body })
    })
}

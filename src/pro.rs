//! What PublishKit Pro adds. The window, the badge and the licence checks
//! are desktop-kit's, the same in every mayorana app.

use dioxus::prelude::*;

/// The badge's tooltip while PublishKit isn't Pro.
pub const HINT: &str = "Pro is the licence for commercial use, with AI credits each month";

/// What the free version does and what Pro adds, in the Pro window.
pub fn pitch() -> Element {
    rsx! {
        p {
            "PublishKit works without a licence for personal and non-commercial use. "
            "Pro is the licence for commercial use, with AI credits each month for api0. "
            "It comes with a monthly plan on mayorana.ch."
        }
    }
}

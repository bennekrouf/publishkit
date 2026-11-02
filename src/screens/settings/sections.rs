//! Sections of the Settings window: the model, the tabs a site shows, and the accounts posts
//! are sent to.

use super::*;
use crate::api0::Region;
use crate::crosspost::Accounts;

/// The model: api0's regions, one click each, and what using one takes.
pub(super) fn model_section(
    region_notes: Vec<(&'static Region, String)>,
    region: Option<&'static Region>,
    has_licence: bool,
    offers_error: Option<String>,
    mut selected_provider: Signal<LlmProvider>,
) -> Element {
    rsx! {
        div { class: "settings-field",
            label { "Model" }
            div { class: "region-row",
                for (r, note) in region_notes {
                    button {
                        key: "{r.id}",
                        class: if region == Some(r) { "region active" } else { "region" },
                        title: "api0, in {r.label}",
                        onclick: move |_| *selected_provider.write() = LlmProvider::api0(r.id),
                        desktop_kit::ui::RegionFlag { id: r.id }
                        span { class: "region-label", "{r.label}" }
                        span { class: "region-note", "{note}" }
                    }
                }
            }
        }

        if let Some(r) = region {
            div { class: "settings-field",
                p { class: "settings-hint",
                    if has_licence {
                        "api0 runs the model in {r.label} and takes AI credits from the licence \
                         shared by every Mayorana app on this computer, with no key to set up. \
                         Which model runs there is api0's choice. A post is about \
                         {crate::api0::TYPICAL_POST_TOKENS} tokens; a translation is two calls."
                    } else {
                        "api0 needs a licence. Paste your licence number in any Mayorana app \
                         (GitAgent, JobScout…) and PublishKit uses it too, or set the "
                        code { "{crate::api0::KEY_ENV}" }
                        " environment variable."
                    }
                }
                if let Some(e) = offers_error {
                    p { class: "settings-hint", "{e}" }
                }
            }
        }
    }
}

/// Whether the site shows its Blog tab, and which networks' tabs.
pub(super) fn tabs_field(mut blog_on: Signal<bool>, mut hidden: Signal<Vec<String>>) -> Element {
    rsx! {
        div { class: "settings-field",
            label { "Tabs" }
            div { class: "settings-checks",
                label { class: "settings-check",
                    input {
                        r#type: "checkbox",
                        checked: blog_on(),
                        onchange: move |e| blog_on.set(e.checked()),
                    }
                    "Blog"
                }
                for channel in Channel::ALL {
                    label { class: "settings-check", key: "{channel.key()}",
                        input {
                            r#type: "checkbox",
                            checked: !hidden.read().iter().any(|h| h == channel.key()),
                            onchange: move |e| {
                                let key = channel.key().to_string();
                                let mut hidden = hidden.write();
                                hidden.retain(|h| *h != key);
                                if !e.checked() {
                                    hidden.push(key);
                                }
                            },
                        }
                        "{channel.label()}"
                    }
                }
            }
            p { class: "settings-hint",
                "Blog shows the Blog and Board tabs; leave it off for a folder of social posts only. \
                 Untick a network you don't post on to hide its tab: its posts stay in the folder."
            }
        }
    }
}

/// The blogging platforms a published post is copied to: dev.to, Hashnode, WordPress, Ghost.
pub(super) fn blog_accounts(mut accounts: Signal<Accounts>) -> Element {
    rsx! {
        p { class: "settings-hint",
            "Send a published post to these platforms from its editor, with your site's address as \
             canonical so search engines credit the original. The keys stay on this computer."
        }
        div { class: "settings-field",
            label { "dev.to API key" }
            input { r#type: "password", placeholder: "dev.to → Settings → Extensions", value: "{accounts.read().devto_key}",
                oninput: move |e| accounts.write().devto_key = e.value() }
        }
        div { class: "settings-field",
            label { "Hashnode" }
            div { class: "meta-row",
                div { class: "field meta-tags",
                    input { r#type: "password", placeholder: "Personal access token", value: "{accounts.read().hashnode_token}",
                        oninput: move |e| accounts.write().hashnode_token = e.value() }
                }
                div { class: "field meta-tags",
                    input { placeholder: "Publication ID", value: "{accounts.read().hashnode_publication}",
                        oninput: move |e| accounts.write().hashnode_publication = e.value() }
                }
            }
        }
        div { class: "settings-field",
            label { "WordPress" }
            input { placeholder: "https://your-wordpress-site.com", value: "{accounts.read().wordpress_url}",
                oninput: move |e| accounts.write().wordpress_url = e.value() }
            div { class: "meta-row",
                div { class: "field meta-tags",
                    input { placeholder: "User name", value: "{accounts.read().wordpress_user}",
                        oninput: move |e| accounts.write().wordpress_user = e.value() }
                }
                div { class: "field meta-tags",
                    input { r#type: "password", placeholder: "Application password", value: "{accounts.read().wordpress_password}",
                        oninput: move |e| accounts.write().wordpress_password = e.value() }
                }
            }
            p { class: "settings-hint", "An application password, from Users → Profile in WordPress — not the one you log in with." }
        }
        div { class: "settings-field",
            label { "Ghost" }
            div { class: "meta-row",
                div { class: "field meta-tags",
                    input { placeholder: "https://your-site.ghost.io", value: "{accounts.read().ghost_url}",
                        oninput: move |e| accounts.write().ghost_url = e.value() }
                }
                div { class: "field meta-tags",
                    input { r#type: "password", placeholder: "Admin API key (id:secret)", value: "{accounts.read().ghost_key}",
                        oninput: move |e| accounts.write().ghost_key = e.value() }
                }
            }
            p { class: "settings-hint", "From Settings → Integrations → Add custom integration in Ghost." }
        }
        label { class: "settings-check",
            input { r#type: "checkbox", checked: accounts.read().as_draft,
                onchange: move |e| accounts.write().as_draft = e.checked() }
            "Send copies as drafts, to publish there after a look"
        }
    }
}

/// The networks a short post goes to from its preview: Bluesky and Mastodon.
pub(super) fn social_accounts(mut accounts: Signal<Accounts>) -> Element {
    rsx! {
        div { class: "settings-field",
            label { "Bluesky" }
            div { class: "meta-row",
                div { class: "field meta-tags",
                    input { placeholder: "you.bsky.social", value: "{accounts.read().bluesky_handle}",
                        oninput: move |e| accounts.write().bluesky_handle = e.value() }
                }
                div { class: "field meta-tags",
                    input { r#type: "password", placeholder: "App password", value: "{accounts.read().bluesky_password}",
                        oninput: move |e| accounts.write().bluesky_password = e.value() }
                }
            }
            p { class: "settings-hint", "An app password from Bluesky's Settings → Privacy and security → App passwords. Posts a post's short teaser from its preview." }
        }
        div { class: "settings-field",
            label { "Mastodon" }
            div { class: "meta-row",
                div { class: "field meta-tags",
                    input { placeholder: "mastodon.social", value: "{accounts.read().mastodon_instance}",
                        oninput: move |e| accounts.write().mastodon_instance = e.value() }
                }
                div { class: "field meta-tags",
                    input { r#type: "password", placeholder: "Access token", value: "{accounts.read().mastodon_token}",
                        oninput: move |e| accounts.write().mastodon_token = e.value() }
                }
            }
            p { class: "settings-hint", "From Preferences → Development → New application, with write:statuses." }
        }
    }
}

#[cfg(test)]
mod tests;

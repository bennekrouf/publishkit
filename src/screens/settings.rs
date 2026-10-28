//! The settings window.

use super::*;

#[component]
pub(super) fn SettingsModal(
    config: Signal<AppConfig>,
    show_settings: Signal<bool>,
    posts: Signal<ProjectPosts>,
) -> Element {
    let cfg = config.read().clone();

    let mut selected_provider = use_signal(|| cfg.provider.clone());
    let mut accounts = use_signal(|| cfg.crosspost.clone());
    let mut saved_msg = use_signal(String::new);
    // What each api0 region costs, asked once when Settings opens.
    let mut offers = use_signal(|| Option::<Result<Vec<crate::api0::RegionOffer>, String>>::None);
    use_future(move || async move {
        offers.set(Some(crate::api0::regions().await));
    });
    let has_licence = crate::api0::licence().is_some();
    let region_notes: Vec<(&'static crate::api0::Region, String)> = crate::api0::REGIONS
        .iter()
        .map(|r| {
            let note = match offers.read().as_ref() {
                _ if !has_licence => "needs a licence".to_string(),
                None => "\u{2026}".to_string(),
                Some(Err(_)) => "unavailable".to_string(),
                Some(Ok(list)) => match list.iter().find(|o| o.id == r.id) {
                    Some(o) => {
                        let n = o.credits_per_post();
                        format!(
                            "\u{2248} {n} {} a post",
                            if n == 1 { "credit" } else { "credits" }
                        )
                    }
                    None => "not offered".to_string(),
                },
            };
            (r, note)
        })
        .collect();
    let region = selected_provider.read().region();
    let offers_error = match offers.read().as_ref() {
        Some(Err(e)) if has_licence => Some(e.clone()),
        _ => None,
    };

    // This site's settings, kept in its publishkit.yaml.
    let project = cfg.project_path.clone().unwrap_or_default();
    let site = use_hook({
        let project = project.clone();
        move || ProjectConfig::load(&project)
    });
    let languages = site.languages();
    let mut site_name = use_signal(|| site.site_name.clone());
    let mut site_url = use_signal(|| site.site_url.clone());
    let mut author = use_signal(|| site.author.clone());
    let mut tags = use_signal(|| site.default_tags.join(", "));
    let mut brief = use_signal(|| site.system_prompt.clone());
    let mut preset_key =
        use_signal(|| site.preset.map(|p| p.key().to_string()).unwrap_or_default());
    let mut publish_cmd = use_signal(|| site.publish_command.clone());
    let mut deploy_cmd = use_signal(|| site.deploy_command.clone());
    let mut deploy_after = use_signal(|| site.deploy_after_publish);
    let mut post_url = use_signal(|| site.post_url.clone());
    let mut layout = use_signal(|| layout_hint(&site));
    let mut images_dir = use_signal(|| site.images(&project).0);
    let mut images_url = use_signal(|| site.images(&project).1);
    let mut lang_code = use_signal(|| languages.first().cloned().unwrap_or_default());
    let single_lang = languages.len() == 1;
    let old_lang = languages.first().cloned().unwrap_or_default();

    rsx! {
        // backdrop
        div {
            class: "modal-backdrop",
            onclick: move |_| *show_settings.write() = false,
        }
        div { class: "modal settings-modal",
            div { class: "modal-head",
                h2 { "Settings" }
                button { class: "modal-close", title: "Close", onclick: move |_| *show_settings.write() = false, "×" }
            }

            div { class: "modal-body",
                // The model: api0's regions, one click each.
                div { class: "settings-field",
                    label { "Model" }
                    div { class: "region-row",
                        for (r, note) in region_notes {
                            button {
                                key: "{r.id}",
                                class: if region == Some(r) { "region active" } else { "region" },
                                title: "api0, in {r.label}",
                                onclick: move |_| *selected_provider.write() = LlmProvider::api0(r.id),
                                crate::api0::RegionFlag { id: r.id }
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

                h3 { class: "settings-section", "This site" }

                div { class: "settings-field",
                    label { "Built with" }
                    select {
                        onchange: {
                            let project = project.clone();
                            move |e: Event<FormData>| {
                                let key = e.value();
                                if let Some(preset) = Preset::from_key(&key) {
                                    let mut preview = ProjectConfig::load(&project);
                                    preview.apply_preset(preset, &project, false);
                                    publish_cmd.set(preview.publish_command.clone());
                                    post_url.set(preview.post_url.clone());
                                    layout.set(layout_hint(&preview));
                                    if preview.images_dir.is_empty() {
                                        let (dir, url) = preview.images(&project);
                                        images_dir.set(dir);
                                        images_url.set(url);
                                    }
                                }
                                preset_key.set(key);
                            }
                        },
                        if preset_key.read().is_empty() {
                            option { value: "", selected: true, "Not recognised yet" }
                        }
                        for preset in Preset::ALL {
                            option { value: preset.key(), selected: preset_key() == preset.key(), "{preset.label()}" }
                        }
                    }
                    p { class: "settings-hint", "{layout}" }
                }

                div { class: "settings-field",
                    label { "Site name" }
                    input { value: "{site_name}", oninput: move |e| site_name.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "Site address" }
                    input { placeholder: "https://example.com", value: "{site_url}", oninput: move |e| site_url.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "A post's address" }
                    input { value: "{post_url}", oninput: move |e| post_url.set(e.value()) }
                    p { class: "settings-hint",
                        "Used in the social teasers, and as the original's address on cross-posted copies. "
                        code { "{{site_url}}" } " " code { "{{lang}}" } " " code { "{{slug}}" } " "
                        code { "{{year}}" } " " code { "{{month}}" } " " code { "{{day}}" } " are filled in."
                    }
                }
                if single_lang {
                    div { class: "settings-field",
                        label { "Language" }
                        input { placeholder: "en", value: "{lang_code}", oninput: move |e| lang_code.set(e.value()) }
                        p { class: "settings-hint", "The code of the language posts are written in, such as en, fr or de." }
                    }
                }
                div { class: "settings-field",
                    label { "Author" }
                    input { value: "{author}", oninput: move |e| author.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "Default tags" }
                    input { placeholder: "comma, separated", value: "{tags}", oninput: move |e| tags.set(e.value()) }
                }
                div { class: "settings-field",
                    label { "Writing brief" }
                    textarea { rows: "5", value: "{brief}", oninput: move |e| brief.set(e.value()) }
                    p { class: "settings-hint", "What the model is told about your site, its readers and its tone." }
                }
                div { class: "settings-field",
                    label { "Images" }
                    div { class: "meta-row",
                        div { class: "field meta-tags",
                            input { placeholder: "public/images", value: "{images_dir}", oninput: move |e| images_dir.set(e.value()) }
                        }
                        div { class: "field meta-tags",
                            input { placeholder: "/images", value: "{images_url}", oninput: move |e| images_url.set(e.value()) }
                        }
                    }
                    p { class: "settings-hint", "The folder images added to a post are copied into, and the address the site serves it at." }
                }
                div { class: "settings-field",
                    label { "After publishing, run" }
                    input { placeholder: "e.g. npm run build", value: "{publish_cmd}", oninput: move |e| publish_cmd.set(e.value()) }
                    p { class: "settings-hint", "A command run in the site's folder each time a post is published. Leave empty for none." }
                }
                div { class: "settings-field",
                    label { "Deploy" }
                    input {
                        placeholder: "e.g. git add -A && git commit -m \"New post\" && git push",
                        value: "{deploy_cmd}",
                        oninput: move |e| deploy_cmd.set(e.value()),
                    }
                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: deploy_after(),
                            onchange: move |e| deploy_after.set(e.checked()),
                        }
                        "Deploy after each publish, scheduled ones included"
                    }
                    p { class: "settings-hint", "What puts the site online, run in its folder by the Deploy button: npm run deploy, vercel --prod, a git push to a host that builds on push…" }
                }

                h3 { class: "settings-section", "Cross-posting" }
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
                label { class: "settings-check",
                    input { r#type: "checkbox", checked: accounts.read().as_draft,
                        onchange: move |e| accounts.write().as_draft = e.checked() }
                    "Send copies as drafts, to publish there after a look"
                }

                if !saved_msg.read().is_empty() {
                    p { class: "settings-saved", "{saved_msg}" }
                }
            }

            div { class: "modal-footer",
                button {
                    class: "btn-primary",
                    onclick: move |_| {
                        let mut cfg = config.write();
                        cfg.provider = selected_provider.read().clone();
                        cfg.crosspost = accounts.read().clone();
                        cfg.save();
                        drop(cfg);

                        if !project.is_empty() {
                            let mut site = ProjectConfig::load(&project);
                            if let Some(preset) = Preset::from_key(&preset_key.read()) {
                                if site.preset != Some(preset) {
                                    site.apply_preset(preset, &project, false);
                                }
                            }
                            site.site_name = site_name.read().trim().to_string();
                            site.site_url = site_url.read().trim().to_string();
                            site.author = author.read().trim().to_string();
                            site.default_tags = tags.read().split(',').map(str::trim)
                                .filter(|t| !t.is_empty()).map(String::from).collect();
                            site.system_prompt = brief.read().trim().to_string();
                            site.publish_command = publish_cmd.read().trim().to_string();
                            site.deploy_command = deploy_cmd.read().trim().to_string();
                            site.deploy_after_publish = deploy_after();
                            site.post_url = post_url.read().trim().to_string();
                            // Kept only when it differs from what the framework would use.
                            let (dir, url) = (images_dir.read().trim().to_string(), images_url.read().trim().to_string());
                            site.images_dir = String::new();
                            site.images_url = String::new();
                            if (dir.clone(), url.clone()) != site.images(&project) {
                                site.images_dir = dir;
                                site.images_url = url;
                            }
                            if single_lang && site.languages() == [old_lang.clone()] {
                                site.rename_language(&old_lang, &lang_code.read());
                            }
                            site.save(&project);
                            layout.set(layout_hint(&site));
                            posts.set(load_posts(&project));
                        }
                        *saved_msg.write() = "Saved ✓".to_string();
                    },
                    "Save"
                }
            }
        }
    }
}

/// Where the posts are, in a line: what a preset found in the site's folder.
pub(super) fn layout_hint(site: &ProjectConfig) -> String {
    let Some(paths) = &site.content_paths else {
        return "No posts found yet.".to_string();
    };
    let multi = paths.langs.len() > 1;
    paths
        .langs
        .iter()
        .map(|(lang, p)| {
            let lang = if multi {
                format!("{}: ", lang.to_uppercase())
            } else {
                String::new()
            };
            match site.drafts {
                Drafts::Folder => format!("{lang}drafts in {}, published in {}", p.queue, p.blog),
                Drafts::Flag => format!("{lang}posts in {}, drafts marked draft: true", p.blog),
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

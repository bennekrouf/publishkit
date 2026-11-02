//! The settings window.

use super::*;

mod sections;

#[component]
pub(super) fn SettingsModal(
    config: Signal<AppConfig>,
    show_settings: Signal<bool>,
    posts: Signal<ProjectPosts>,
) -> Element {
    let cfg = config.read().clone();

    let selected_provider = use_signal(|| cfg.provider.clone());
    let accounts = use_signal(|| cfg.crosspost.clone());
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
    let blog_on = use_signal(|| site.blog);
    let hidden = use_signal(|| site.hidden_networks.clone());
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
                {sections::model_section(region_notes, region, has_licence, offers_error, selected_provider)}

                h3 { class: "settings-section", if blog_on() { "This site" } else { "This folder" } }

                {sections::tabs_field(blog_on, hidden)}

                if blog_on() {
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
                }

                div { class: "settings-field",
                    label { if blog_on() { "Site name" } else { "Name" } }
                    input { value: "{site_name}", oninput: move |e| site_name.set(e.value()) }
                    if !blog_on() {
                        p { class: "settings-hint", "Yours or your brand's: the model writes the posts for it." }
                    }
                }
                if blog_on() {
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
                }
                if single_lang {
                    div { class: "settings-field",
                        label { "Language" }
                        input { placeholder: "en", value: "{lang_code}", oninput: move |e| lang_code.set(e.value()) }
                        p { class: "settings-hint", "The code of the language posts are written in, such as en, fr or de." }
                    }
                }
                if blog_on() {
                    div { class: "settings-field",
                        label { "Author" }
                        input { value: "{author}", oninput: move |e| author.set(e.value()) }
                    }
                    div { class: "settings-field",
                        label { "Default tags" }
                        input { placeholder: "comma, separated", value: "{tags}", oninput: move |e| tags.set(e.value()) }
                    }
                }
                div { class: "settings-field",
                    label { "Writing brief" }
                    textarea { rows: "5", value: "{brief}", oninput: move |e| brief.set(e.value()) }
                    p { class: "settings-hint",
                        if blog_on() { "What the model is told about your site, its readers and its tone." }
                        else { "What the model is told about you, your audience and your tone." }
                    }
                }
                if blog_on() {
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
                }

                h3 { class: "settings-section", "Cross-posting" }
                if blog_on() {
                    {sections::blog_accounts(accounts)}
                }
                {sections::social_accounts(accounts)}

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
                            site.blog = blog_on();
                            site.hidden_networks = hidden.read().clone();
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

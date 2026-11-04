//! The Deploy button's dialog: what it puts online, the command it runs, and
//! whether that also happens on its own after each publish.
//!
//! The button used to run the command at once, without a word about it, or
//! with no command set, to open the whole Settings window. This asks about
//! the one action instead, and takes the command right here when there is
//! none yet.

use super::*;

/// What the dialog says about the site, read when the button is pressed.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct DeployView {
    /// The site's name, for the heading.
    pub(super) site: String,
    /// The folder the command runs in: the site's own.
    pub(super) folder: String,
    /// The command saved for this site; empty when none is set yet.
    pub(super) saved: String,
}

/// Commands to suggest when none is set yet.
pub(super) const EXAMPLES: [&str; 3] = ["git push", "npm run deploy", "vercel --prod"];

/// The dialog. `draft` is the command typed when none is saved; `after` is
/// the "after each publish" setting, saved as it is ticked.
pub(super) fn deploy_dialog(
    view: &DeployView,
    mut draft: Signal<String>,
    after: Signal<bool>,
    on_deploy: EventHandler<()>,
    on_cancel: EventHandler<()>,
    on_after: EventHandler<bool>,
    on_settings: EventHandler<()>,
) -> Element {
    let asking = view.saved.trim().is_empty();
    let command = if asking {
        draft.read().trim().to_string()
    } else {
        view.saved.trim().to_string()
    };
    let automatic = if after() {
        "On: it also runs by itself after each post is published, scheduled ones included."
    } else {
        "Off: it runs only when you press Deploy."
    };
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_cancel.call(()) }
        div { class: "modal deploy-modal",
            div { class: "modal-head",
                h2 { "Deploy {view.site}" }
                button { class: "modal-close", title: "Close", onclick: move |_| on_cancel.call(()), "×" }
            }
            div { class: "modal-body",
                div { class: "settings-field",
                    label { "Site folder" }
                    code { class: "deploy-value", "{view.folder}" }
                    p { class: "settings-hint", "This is the folder that is put online." }
                }
                div { class: "settings-field",
                    label { "Command" }
                    if asking {
                        input {
                            class: "deploy-command",
                            placeholder: "e.g. git push",
                            value: "{draft}",
                            autofocus: true,
                            spellcheck: "false",
                            oninput: move |e| draft.set(e.value()),
                        }
                        p { class: "settings-hint",
                            "No deploy command is set for this site yet. Type the one you would run in a terminal, in the site\u{2019}s folder, to put it online. For example: "
                            for (i, example) in EXAMPLES.iter().enumerate() {
                                if i > 0 { ", " }
                                code { "{example}" }
                            }
                            ". It is saved for next time."
                        }
                    } else {
                        code { class: "deploy-value", "{command}" }
                    }
                    p { class: "settings-hint",
                        "Deploy now runs it once, now, on this computer, in the site folder."
                    }
                }
                div { class: "settings-field",
                    label { "After each publish" }
                    label { class: "settings-check",
                        input {
                            r#type: "checkbox",
                            checked: after(),
                            onchange: move |e| on_after.call(e.checked()),
                        }
                        "Also deploy automatically after each publish"
                    }
                    p { class: "settings-hint deploy-auto", "{automatic}" }
                }
            }
            div { class: "modal-footer deploy-footer",
                button {
                    class: "btn-link deploy-settings",
                    title: "Open Settings for everything else about this site",
                    onclick: move |_| on_settings.call(()),
                    "All settings\u{2026}"
                }
                button { class: "btn-small", onclick: move |_| on_cancel.call(()), "Cancel" }
                button {
                    class: "btn-primary",
                    disabled: command.is_empty(),
                    title: if command.is_empty() { "Type the command first".to_string() } else { format!("Runs {command}") },
                    onclick: move |_| on_deploy.call(()),
                    "Deploy now"
                }
            }
        }
    }
}

/// The Deploy button and its dialog.
#[component]
pub(super) fn DeployControl(
    config: Signal<AppConfig>,
    deploying: Signal<bool>,
    notice: Signal<String>,
    show_settings: Signal<bool>,
    command: String,
) -> Element {
    let mut open = use_signal(|| Option::<DeployView>::None);
    let mut draft = use_signal(String::new);
    let mut after = use_signal(|| false);
    let project = move || config.read().project_path.clone().unwrap_or_default();
    rsx! {
        button {
            class: "kit-btn kit-deploy",
            disabled: deploying(),
            title: if command.is_empty() { "Set the command that puts your site online".to_string() } else { format!("Runs {command}, after asking") },
            onclick: move |_| {
                let path = project();
                let site = ProjectConfig::load(&path);
                draft.set(String::new());
                after.set(site.deploy_after_publish);
                open.set(Some(DeployView {
                    site: crate::state::site_label(&path),
                    folder: path,
                    saved: site.deploy_command,
                }));
            },
            if deploying() { "Deploying…" } else { "Deploy" }
        }
        if let Some(view) = open.read().clone() {
            {deploy_dialog(
                &view,
                draft,
                after,
                EventHandler::new(move |_| {
                    let path = project();
                    let mut site = ProjectConfig::load(&path);
                    if site.deploy_command.trim().is_empty() {
                        let typed = draft.peek().trim().to_string();
                        if typed.is_empty() {
                            return;
                        }
                        site.deploy_command = typed;
                        site.save(&path);
                    }
                    open.set(None);
                    run_deploy(path, site, deploying, notice);
                }),
                EventHandler::new(move |_| open.set(None)),
                EventHandler::new(move |on: bool| {
                    after.set(on);
                    let path = project();
                    let mut site = ProjectConfig::load(&path);
                    site.deploy_after_publish = on;
                    site.save(&path);
                }),
                EventHandler::new(move |_| {
                    open.set(None);
                    show_settings.set(true);
                }),
            )}
        }
    }
}

/// Runs the site's deploy command once, saying how it went.
fn run_deploy(
    project: String,
    site: ProjectConfig,
    mut deploying: Signal<bool>,
    mut notice: Signal<String>,
) {
    deploying.set(true);
    notice.set(format!("Deploying with {}…", site.deploy_command));
    spawn(async move {
        let run = tokio::task::spawn_blocking(move || api::deploy(&project, &site))
            .await
            .ok()
            .flatten();
        notice.set(match run {
            Some(run) if run.ok => format!("Deployed: {} ran.", run.command),
            Some(run) => format!("The deploy failed: {}", run.output),
            None => String::new(),
        });
        deploying.set(false);
    });
}

#[cfg(test)]
mod tests;

//! The editor for one post: its front matter as fields, its body as markdown, saved as you
//! type. Images go into the site's images folder; a post can be translated into the site's
//! other languages as new drafts.

use crate::api;
use crate::frontmatter::{self, Field, Format};
use crate::plan::{self, Stage};
use crate::state::{language_name, load_posts, AppConfig, Post, ProjectConfig, ProjectPosts};
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use std::path::PathBuf;

/// How long typing pauses before the post is saved.
const AUTOSAVE_AFTER: std::time::Duration = std::time::Duration::from_millis(1200);

/// Words a reader gets through in a minute, for the reading time.
const WORDS_PER_MINUTE: usize = 225;

/// The front matter fields the editor shows.
#[derive(Clone, Debug, PartialEq, Default)]
struct Meta {
    title: String,
    description: String,
    date: String,
    tags: String,
}

impl Meta {
    fn of(content: &str) -> Self {
        let f = frontmatter::fields(content);
        Meta {
            title: frontmatter::text(&f, "title").unwrap_or_default(),
            description: frontmatter::text(&f, api::description_key(content)).unwrap_or_default(),
            date: frontmatter::text(&f, "date").unwrap_or_default(),
            tags: frontmatter::list(&f, "tags").join(", "),
        }
    }
}

/// The post as it was loaded (`base`), with the fields that differ from `loaded` set and the
/// body replaced. A field left alone keeps its own spelling: a folded description, a date and
/// time, a list written one item per line.
fn compose(base: &str, loaded: &Meta, meta: &Meta, body: &str, default: Format) -> String {
    let format = frontmatter::split(base).map_or(default, |p| p.format);
    let text = |s: &str| Field::Text(s.trim().to_string());
    let mut out = base.to_string();
    if meta.title != loaded.title {
        out = frontmatter::set(&out, "title", &text(&meta.title), format);
    }
    if meta.description != loaded.description {
        out = frontmatter::set(&out, api::description_key(base), &text(&meta.description), format);
    }
    if meta.date != loaded.date {
        out = frontmatter::set(&out, "date", &text(&meta.date), format);
    }
    if meta.tags != loaded.tags {
        let tags = meta.tags.split(',').map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect();
        out = frontmatter::set(&out, "tags", &Field::List(tags), format);
    }
    frontmatter::with_body(&out, body)
}

/// Formatting for the body (`window.pkFormat('bold')` and so on, from the toolbar), Cmd/Ctrl+B,
/// I and K on it, and images pasted into it, handed to the editor through `sink` (the latest
/// editor's channel: the listeners are added once, the channel follows the editor).
const EDITOR_JS: &str = r#"
window.pkFormat = function (kind) {
  const t = document.getElementById('md-editor');
  if (!t) return;
  const s = t.selectionStart, e = t.selectionEnd, v = t.value, sel = v.slice(s, e);
  const wrap = (a, b, placeholder) => {
    const text = sel || placeholder;
    t.setRangeText(a + text + b, s, e, 'select');
    t.selectionStart = s + a.length;
    t.selectionEnd = s + a.length + text.length;
  };
  const lines = (prefix) => {
    const from = v.lastIndexOf('\n', s - 1) + 1;
    let to = v.indexOf('\n', e > s ? e - 1 : e);
    if (to < 0) to = v.length;
    const all = v.slice(from, to).split('\n');
    const on = all.every(l => l.startsWith(prefix));
    const block = all.map(l => on ? l.slice(prefix.length) : prefix + l.replace(/^(#{1,6} |- |> )/, '')).join('\n');
    t.setRangeText(block, from, to, 'select');
  };
  switch (kind) {
    case 'bold': wrap('**', '**', 'bold text'); break;
    case 'italic': wrap('_', '_', 'text'); break;
    case 'code': sel.includes('\n') ? wrap('```\n', '\n```', 'code') : wrap('`', '`', 'code'); break;
    case 'link': wrap('[', '](https://)', 'link text'); break;
    case 'h2': lines('## '); break;
    case 'h3': lines('### '); break;
    case 'list': lines('- '); break;
    case 'quote': lines('> '); break;
  }
  t.focus();
  t.dispatchEvent(new Event('input', { bubbles: true }));
};
window.pkPasteSink = dioxus;
if (!window.pkEditorListening) {
  window.pkEditorListening = true;
  document.addEventListener('keydown', (e) => {
    if (e.target.id !== 'md-editor' || !(e.metaKey || e.ctrlKey) || e.altKey) return;
    const kind = { b: 'bold', i: 'italic', k: 'link' }[e.key.toLowerCase()];
    if (kind) { e.preventDefault(); window.pkFormat(kind); }
  });
  document.addEventListener('paste', (e) => {
    if (e.target.id !== 'md-editor') return;
    // WebKit hands a pasted image over as an item rather than in `files`.
    const fromItems = [...(e.clipboardData?.items || [])]
      .filter(i => i.kind === 'file' && i.type.startsWith('image/'))
      .map(i => i.getAsFile())
      .filter(Boolean);
    const fromFiles = [...(e.clipboardData?.files || [])].filter(f => f.type.startsWith('image/'));
    const images = fromItems.length ? fromItems : fromFiles;
    if (!images.length) return;
    e.preventDefault();
    for (const file of images) {
      const reader = new FileReader();
      reader.onload = () => window.pkPasteSink.send({ type: file.type, data: reader.result.split(',')[1] });
      reader.readAsDataURL(file);
    }
  });
}
// The channel stays open while the editor is: pasted images come through it.
await new Promise(() => {});
"#;

/// The toolbar's buttons: what each applies, its label, and its tooltip.
const FORMATS: [(&str, &str, &str); 8] = [
    ("bold", "B", "Bold (⌘B)"),
    ("italic", "I", "Italic (⌘I)"),
    ("link", "Link", "Link (⌘K)"),
    ("h2", "H2", "Section heading"),
    ("h3", "H3", "Sub-heading"),
    ("list", "•", "List"),
    ("quote", "❝", "Quote"),
    ("code", "</>", "Code"),
];

/// `block` put into the body where the cursor is, on lines of its own (an image, a figure),
/// as if the person had typed it.
fn insert_block(block: &str) {
    let block = serde_json::to_string(block).unwrap_or_default();
    let _ = document::eval(&format!(
        "const t = document.getElementById('md-editor');
         if (t) {{
           t.focus();
           const before = t.value.slice(0, t.selectionStart);
           const after = t.value.slice(t.selectionEnd);
           const lead = before === '' || before.endsWith('\\n\\n') ? '' : before.endsWith('\\n') ? '\\n' : '\\n\\n';
           const tail = after.startsWith('\\n\\n') ? '' : after.startsWith('\\n') ? '\\n' : '\\n\\n';
           t.setRangeText(lead + {block} + tail, t.selectionStart, t.selectionEnd, 'end');
           t.dispatchEvent(new Event('input', {{ bubbles: true }}));
         }}"
    ));
}

#[component]
pub fn PostEditor(
    config: Signal<AppConfig>,
    posts: Signal<ProjectPosts>,
    selected: Signal<Option<Post>>,
) -> Element {
    let mut loaded_path = use_signal(|| None::<PathBuf>);
    let mut base        = use_signal(String::new);
    let mut loaded      = use_signal(Meta::default);
    let mut meta        = use_signal(Meta::default);
    let mut body        = use_signal(String::new);
    // Edits made, and how many of them are on disk.
    let mut edits       = use_signal(|| 0u64);
    let mut saved       = use_signal(|| 0u64);
    let mut status      = use_signal(String::new);
    let mut busy        = use_signal(|| false);
    let mut link_issues = use_signal(Vec::<String>::new);
    let mut sending = use_signal(|| None::<crate::crosspost::Target>);
    let mut checking_links = use_signal(|| false);

    let project = move || config.peek().project_path.clone().unwrap_or_default();

    let mut save = move || {
        let Some(path) = loaded_path.peek().clone() else { return };
        let project = project();
        let pcfg = ProjectConfig::load(&project);
        let content = compose(&base.peek(), &loaded.peek(), &meta.peek(), &body.peek(), pcfg.front_matter);
        match std::fs::write(&path, &content) {
            Ok(()) => {
                saved.set(*edits.peek());
                status.set(format!("Saved at {}", chrono::Local::now().format("%H:%M")));
                posts.set(load_posts(&project));
            }
            Err(e) => status.set(format!("Couldn't save: {e}")),
        }
    };

    // Another post picked: what is still unsaved of this one is saved, then that one loads.
    use_effect(move || {
        let Some(post) = selected.read().clone() else { return };
        if loaded_path.peek().as_ref() == Some(&post.path) {
            return;
        }
        if *edits.peek() > *saved.peek() {
            save();
        }
        loaded_path.set(Some(post.path.clone()));
        base.set(post.full_content.clone());
        let m = Meta::of(&post.full_content);
        loaded.set(m.clone());
        meta.set(m);
        body.set(post.body.clone());
        edits.set(0);
        saved.set(0);
        status.set(String::new());
        link_issues.set(vec![]);
    });

    // After each change: the preview follows at once, the file once typing pauses.
    let mut changed = move || {
        let n = *edits.peek() + 1;
        edits.set(n);
        let pcfg_format = ProjectConfig::load(&project()).front_matter;
        let content = compose(&base.peek(), &loaded.peek(), &meta.peek(), &body.peek(), pcfg_format);
        let current = selected.peek().clone();
        if let Some(mut p) = current {
            p.full_content = content;
            p.title = meta.peek().title.clone();
            selected.set(Some(p));
        }
        spawn(async move {
            tokio::time::sleep(AUTOSAVE_AFTER).await;
            if *edits.peek() == n {
                save();
            }
        });
    };

    let mut add_images = move |paths: Vec<PathBuf>| {
        let project = project();
        let pcfg = ProjectConfig::load(&project);
        let post = loaded_path.peek().clone().unwrap_or_default();
        for path in paths {
            match api::add_image(&project, &pcfg, &path, &post) {
                Ok(url) => {
                    let alt = path.file_stem().map(|s| s.to_string_lossy().replace(['-', '_'], " ")).unwrap_or_default();
                    insert_block(&format!("![{alt}]({url})"));
                    status.set(format!("Added {url}"));
                }
                Err(e) => status.set(format!("{e:#}")),
            }
        }
    };

    // Images pasted into the body arrive here, as the browser read them; they're written to a
    // file of their own, then added like a dropped image.
    use_future(move || async move {
        let mut channel = document::eval(EDITOR_JS);
        loop {
            let pasted = match channel.recv::<serde_json::Value>().await {
                Ok(pasted) => pasted,
                Err(e) => {
                    status.set(format!("The editor's tools didn't start: {e}"));
                    break;
                }
            };
            use base64::Engine;
            let Some(bytes) = pasted["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()) else { continue };
            let extension = match pasted["type"].as_str().unwrap_or("") {
                "image/jpeg" => "jpg", "image/gif" => "gif", "image/webp" => "webp", "image/svg+xml" => "svg", _ => "png",
            };
            let dir = std::env::temp_dir().join("publishkit-pasted");
            let _ = std::fs::create_dir_all(&dir);
            let file = dir.join(format!("pasted-{}.{extension}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
            match std::fs::write(&file, bytes) {
                Ok(()) => add_images(vec![file]),
                Err(e) => status.set(format!("Couldn't keep the pasted image: {e}")),
            }
        }
    });

    let words = body.read().split_whitespace().count();
    let minutes = words.div_ceil(WORDS_PER_MINUTE).max(1);
    let post_lang = selected.read().as_ref().map(|p| p.lang.clone()).unwrap_or_default();
    let others: Vec<String> = posts.read().languages.iter().filter(|l| **l != post_lang).cloned().collect();
    let unsaved = *edits.read() > *saved.read();
    let issues = selected.read().as_ref().map(|post| {
        let project = project();
        crate::checks::check(post, &project, &ProjectConfig::load(&project))
    }).unwrap_or_default();
    let draft = selected.read().as_ref().is_some_and(|p| p.is_draft());
    let stage = selected.read().as_ref().map(|p| p.stage).unwrap_or_default();
    let publish_at = selected.read().as_ref().and_then(|p| p.publish_at.clone()).unwrap_or_default();

    // A draft's stage and date live in the site's plan, not in its file: saved at once.
    let mut plan_change = move |change: Box<dyn FnOnce(&mut plan::Entry)>| {
        let Some(post) = selected.peek().clone() else { return };
        let project = project();
        if let Err(e) = plan::update(&project, &post.lang, &post.filename, change) {
            status.set(format!("Couldn't save the schedule: {e}"));
            return;
        }
        let entry = plan::Plan::load(&project).get(&post.lang, &post.filename);
        let mut post = post;
        post.stage = entry.stage;
        post.publish_at = entry.publish_at;
        selected.set(Some(post));
        posts.set(load_posts(&project));
    };

    rsx! {
        div { class: "editor-wrap",
            div { class: "field",
                label { "Title" }
                input {
                    value: "{meta.read().title}",
                    oninput: move |e| { meta.write().title = e.value(); changed(); },
                }
            }
            div { class: "field",
                label { "Description" }
                textarea {
                    rows: "2",
                    value: "{meta.read().description}",
                    oninput: move |e| { meta.write().description = e.value(); changed(); },
                }
            }
            div { class: "meta-row",
                div { class: "field",
                    label { "Date" }
                    input {
                        placeholder: "2026-01-31",
                        value: "{meta.read().date}",
                        oninput: move |e| { meta.write().date = e.value(); changed(); },
                    }
                }
                div { class: "field meta-tags",
                    label { "Tags" }
                    input {
                        placeholder: "comma, separated",
                        value: "{meta.read().tags}",
                        oninput: move |e| { meta.write().tags = e.value(); changed(); },
                    }
                }
            }

            if draft {
                div { class: "meta-row",
                    div { class: "field",
                        label { "Stage" }
                        select {
                            onchange: move |e: Event<FormData>| {
                                if let Some(s) = Stage::from_key(&e.value()) {
                                    plan_change(Box::new(move |entry| entry.stage = s));
                                }
                            },
                            for s in Stage::ALL {
                                option { value: s.key(), selected: s == stage, "{s.label()}" }
                            }
                        }
                    }
                    div { class: "field meta-tags",
                        label { "Publish on" }
                        div { class: "schedule-row",
                            input {
                                r#type: "datetime-local",
                                value: "{publish_at}",
                                onchange: move |e: Event<FormData>| {
                                    let when = plan::parse_when(&e.value()).map(plan::format_when);
                                    plan_change(Box::new(move |entry| entry.publish_at = when));
                                },
                            }
                            if !publish_at.is_empty() {
                                button {
                                    class: "btn-small",
                                    title: "Don't publish it automatically",
                                    onclick: move |_| plan_change(Box::new(|entry| entry.publish_at = None)),
                                    "✕"
                                }
                            }
                        }
                    }
                }
            }

            div { class: "editor-toolbar",
                button {
                    class: "btn-small",
                    title: "Copy an image into the site and link it where the cursor is. You can also drop images on the text.",
                    onclick: move |_| {
                        spawn(async move {
                            if let Some(files) = rfd::AsyncFileDialog::new()
                                .set_title("Add images")
                                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "svg", "avif"])
                                .pick_files()
                                .await
                            {
                                add_images(files.iter().map(|f| f.path().to_path_buf()).collect());
                            }
                        });
                    },
                    "Image…"
                }
                for lang in others {
                    button {
                        class: "btn-small",
                        disabled: busy(),
                        title: "Translate into {language_name(&lang)} as a new draft",
                        onclick: {
                            let lang = lang.clone();
                            move |_| {
                                let to = lang.clone();
                                if *edits.peek() > *saved.peek() {
                                    save();
                                }
                                let Some(post) = selected.peek().clone() else { return };
                                let cfg = config.peek().clone();
                                let key = cfg.resolved_api_key();
                                if key.is_empty() {
                                    status.set(cfg.missing_key());
                                    return;
                                }
                                let project = project();
                                busy.set(true);
                                status.set(format!("Translating into {}…", language_name(&to)));
                                spawn(async move {
                                    let pcfg = ProjectConfig::load(&project);
                                    let content = std::fs::read_to_string(&post.path).unwrap_or(post.full_content.clone());
                                    let done = api::translate_post(
                                        &project, &pcfg, &content, &post.filename, &post.lang, &to, &cfg.provider, &key,
                                    ).await;
                                    match done {
                                        Ok(path) => {
                                            status.set(format!(
                                                "{} draft saved: {}",
                                                language_name(&to),
                                                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                                            ));
                                            posts.set(load_posts(&project));
                                        }
                                        Err(e) => status.set(format!("Couldn't translate: {e:#}")),
                                    }
                                    busy.set(false);
                                });
                            }
                        },
                        "→ {lang.to_uppercase()}"
                    }
                }
                span { class: "editor-stats", "{words} words · {minutes} min read" }
            }

            if !draft {
                {
                    use crate::crosspost::{Record, Target};
                    let accounts = config.read().crosspost.clone();
                    let targets: Vec<Target> = Target::ALL.into_iter().filter(|t| t.configured(&accounts)).collect();
                    let (lang, filename) = selected.read().as_ref().map(|p| (p.lang.clone(), p.filename.clone())).unwrap_or_default();
                    let copies = Record::load(&project()).copies(&lang, &filename);
                    rsx! {
                        div { class: "crosspost-row",
                            span { class: "crosspost-label", "Cross-post" }
                            if targets.is_empty() {
                                span { class: "settings-hint", "Add dev.to, Hashnode, WordPress or Ghost in ⚙ Settings to send copies." }
                            }
                            for target in targets {
                                {
                                    let copy = copies.get(&target).cloned();
                                    let verb = if copy.is_some() { "Update" } else { "Send to" };
                                    let how = if accounts.as_draft { "as a draft" } else { "published" };
                                    rsx! {
                                        button {
                                            class: "btn-small",
                                            disabled: sending().is_some(),
                                            title: "{verb} {target.label()} {how}, with this post's address as canonical",
                                            onclick: move |_| {
                                                if *edits.peek() > *saved.peek() {
                                                    save();
                                                }
                                                let Some(post) = selected.peek().clone() else { return };
                                                let project = project();
                                                let accounts = config.peek().crosspost.clone();
                                                sending.set(Some(target));
                                                status.set(format!("Sending to {}…", target.label()));
                                                spawn(async move {
                                                    let pcfg = ProjectConfig::load(&project);
                                                    let content = std::fs::read_to_string(&post.path).unwrap_or(post.full_content.clone());
                                                    let article = crate::crosspost::Article::of(
                                                        &content, &pcfg.site_url, &pcfg.address(&post.lang, &post.filename, &content),
                                                    );
                                                    let done = crate::crosspost::crosspost(
                                                        &project, &accounts, target, &post.lang, &post.filename, &article,
                                                    ).await;
                                                    status.set(match done {
                                                        Ok(copy) => format!("On {}: {}", target.label(), copy.url),
                                                        Err(e) => format!("Couldn't send to {}: {e:#}", target.label()),
                                                    });
                                                    sending.set(None);
                                                });
                                            },
                                            if sending() == Some(target) { "Sending…" }
                                            else if copy.is_some() { "↻ {target.label()}" }
                                            else { "{target.label()}" }
                                        }
                                        if let Some(copy) = copy.filter(|c| !c.url.is_empty()) {
                                            a { class: "crosspost-link", href: "{copy.url}", target: "_blank", title: "{copy.url}", "↗" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "checks",
                if issues.is_empty() {
                    span { class: "checks-ok", "✓ Nothing to fix" }
                }
                for issue in issues {
                    div {
                        class: if issue.level == crate::checks::Level::Problem { "check problem" } else { "check warning" },
                        "{issue.message}"
                    }
                }
                for message in link_issues() {
                    div { class: "check warning", "{message}" }
                }
                button {
                    class: "btn-micro",
                    disabled: checking_links(),
                    title: "Open each link to another site and report the ones that fail",
                    onclick: move |_| {
                        let text = body.peek().clone();
                        checking_links.set(true);
                        link_issues.set(vec![]);
                        spawn(async move {
                            let found = crate::checks::broken_links(&text).await;
                            link_issues.set(if found.is_empty() {
                                vec![]
                            } else {
                                found.into_iter().map(|i| i.message).collect()
                            });
                            status.set(if link_issues.peek().is_empty() { "Every link answers.".to_string() } else { String::new() });
                            checking_links.set(false);
                        });
                    },
                    if checking_links() { "Checking links…" } else { "Check links" }
                }
            }

            div { class: "format-bar",
                for (kind, label, tip) in FORMATS {
                    button {
                        class: "format-btn format-{kind}",
                        title: "{tip}",
                        // Keeps the selection in the text while the button is pressed.
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| {
                            spawn(async move {
                                if let Err(e) = document::eval(&format!("window.pkFormat('{kind}')")).await {
                                    status.set(format!("Couldn't format: {e}"));
                                }
                            });
                        },
                        "{label}"
                    }
                }
                span { class: "format-hint", "Paste or drop images into the text" }
            }

            textarea {
                id: "md-editor",
                class: "md-editor",
                value: "{body}",
                spellcheck: "false",
                oninput: move |e| { body.set(e.value()); changed(); },
                ondragover: move |e| e.prevent_default(),
                ondrop: move |e| {
                    e.prevent_default();
                    add_images(e.files().iter().map(|f| f.path()).collect());
                },
            }

            div { class: "editor-footer",
                p { class: "status-msg",
                    if unsaved { "Unsaved changes…" } else { "{status}" }
                }
                button {
                    class: "btn-primary",
                    disabled: !unsaved,
                    onclick: move |_| save(),
                    "Save"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_script_keeps_its_line_breaks_escaped() {
        // A string literal broken over two lines stops the whole script, and every tool with it.
        for line in EDITOR_JS.lines() {
            let quotes = line.matches('\'').count() - line.matches("\\'").count();
            assert!(quotes % 2 == 0, "a quote left open: {line}");
        }
        assert!(EDITOR_JS.contains(r"lastIndexOf('\n'"));
    }

    #[test]
    fn only_the_fields_changed_are_rewritten() {
        let base = "---\ntitle: \"A\"\nexcerpt: >-\n  Folded\n  text.\ndate: '2026-01-01'\ntags:\n  - x\n---\n\nOld body";
        let loaded = Meta::of(base);
        assert_eq!(loaded.description, "Folded text.");
        assert_eq!(loaded.tags, "x");

        let same = compose(base, &loaded, &loaded, "New body", Format::Yaml);
        assert_eq!(same, base.replace("Old body", "New body"));

        let meta = Meta { description: "Shorter.".into(), tags: "x, y".into(), ..loaded.clone() };
        let out = compose(base, &loaded, &meta, "Old body", Format::Yaml);
        assert_eq!(out, "---\ntitle: \"A\"\nexcerpt: \"Shorter.\"\ndate: '2026-01-01'\ntags: [\"x\", \"y\"]\n---\n\nOld body");
    }
}

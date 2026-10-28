//! A site's settings: where its posts, images and builds are.

use super::*;

impl ProjectConfig {
    /// Defaults for a site PublishKit hasn't seen: named after its folder.
    pub(super) fn neutral(project_path: &str) -> Self {
        let site_name = Path::new(project_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            site_name,
            ..Self::default()
        }
    }

    pub fn load(project_path: &str) -> Self {
        let path = config_path(project_path);
        let file = std::fs::read_to_string(&path).ok();
        let (mut config, writable) = match file.as_deref().map(serde_yaml::from_str::<Self>) {
            Some(Ok(config)) => (config, true),
            // Never written over: a mistake in it is the person's to fix, not ours to erase.
            Some(Err(_)) => (Self::neutral(project_path), false),
            None => (Self::neutral(project_path), true),
        };
        if config.preset.is_none() {
            let legacy = file.is_some() && writable && config.content_paths.is_some();
            let preset = if legacy {
                Some(Preset::PublishKit)
            } else {
                detect_preset(project_path)
            };
            if let Some(preset) = preset {
                config.apply_preset(preset, project_path, legacy);
                // Persist detection to yaml so it only runs once
                if writable {
                    config.save(project_path);
                }
            }
        }
        config
    }

    pub fn save(&self, project_path: &str) {
        let path = config_path(project_path);
        if let Ok(yaml) = serde_yaml::to_string(self) {
            let _ = std::fs::write(path, yaml);
        }
    }

    /// Sets the folders and conventions of `preset`, found in the site's folder. `keep_paths`
    /// leaves the folders as they are configured.
    pub fn apply_preset(&mut self, preset: Preset, project_path: &str, keep_paths: bool) {
        let root = Path::new(project_path);
        self.preset = Some(preset);
        self.linkedin_dir = String::new();
        self.date_in_filename = false;

        if preset == Preset::PublishKit {
            if !keep_paths {
                let found = detect_publishkit_layout(root);
                self.content_paths = Some(if found.langs.is_empty() {
                    let lang = "en".to_string();
                    let paths = LangPaths {
                        queue: "content/en/queue".into(),
                        blog: "content/en/blog".into(),
                        suffix: String::new(),
                    };
                    ContentPaths {
                        langs: BTreeMap::from([(lang, paths)]),
                    }
                } else {
                    found
                });
            }
            self.drafts = Drafts::Folder;
            self.front_matter = Format::Yaml;
            self.extension = "md".into();
            self.title_heading = true;
            self.publish_command = if root.join(PUBLISHKIT_INDEX_SCRIPT).is_file() {
                format!("node {PUBLISHKIT_INDEX_SCRIPT}")
            } else {
                String::new()
            };
            self.post_url = "{site_url}/{lang}/blog/{slug}".into();
            return;
        }

        let spec = spec(preset);
        self.drafts = if spec.drafts_dir.is_some() {
            Drafts::Folder
        } else {
            Drafts::Flag
        };
        self.title_heading = false;
        self.date_in_filename = preset == Preset::Jekyll;
        self.publish_command = String::new();
        self.post_url = spec.post_url.into();
        // Kept out of the site's own content, where a framework would publish them as pages.
        self.linkedin_dir = ".publishkit/linkedin".into();
        if !keep_paths {
            let langs = find_posts_dirs(root, &spec);
            self.content_paths = Some(ContentPaths { langs });
        }
        let blog = self
            .languages()
            .first()
            .map(|l| self.resolve_paths(project_path, l).1);
        let sample = blog.as_deref().and_then(first_post);
        self.front_matter = match sample.as_deref().map(std::fs::read_to_string) {
            Some(Ok(c)) if c.starts_with("+++") => Format::Toml,
            Some(Ok(c)) if c.starts_with("---") => Format::Yaml,
            _ => spec.front_matter,
        };
        self.extension = match sample.as_deref().and_then(|p| p.extension()) {
            Some(e) if e == "mdx" => "mdx".into(),
            _ => "md".into(),
        };
        self.bundles = sample.as_deref().is_some_and(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_bundle_index)
        });
    }

    /// Get resolved paths for a language, falling back to default layout.
    pub fn resolve_paths(&self, project_path: &str, lang: &str) -> (PathBuf, PathBuf) {
        if let Some(cp) = &self.content_paths {
            if let Some(lp) = cp.langs.get(lang) {
                return (
                    PathBuf::from(project_path).join(native(&lp.queue)),
                    PathBuf::from(project_path).join(native(&lp.blog)),
                );
            }
        }
        // Default fallback
        let root = PathBuf::from(project_path).join("content").join(lang);
        (root.join("queue"), root.join("blog"))
    }

    /// Paths for standalone LinkedIn posts: `{linkedin_dir}/{lang}/{queue,published}`, or
    /// content/{lang}/linkedin/{queue,published}.
    pub fn resolve_linkedin_paths(&self, project_path: &str, lang: &str) -> (PathBuf, PathBuf) {
        let root = if self.linkedin_dir.is_empty() {
            PathBuf::from(project_path)
                .join("content")
                .join(lang)
                .join("linkedin")
        } else {
            PathBuf::from(project_path)
                .join(native(&self.linkedin_dir))
                .join(lang)
        };
        (root.join("queue"), root.join("published"))
    }

    /// This language's mark in file names (`fr` for `post.fr.md`); empty when it has none.
    pub fn language_mark(&self, lang: &str) -> &str {
        self.content_paths
            .as_ref()
            .and_then(|c| c.langs.get(lang))
            .map_or("", |p| p.suffix.as_str())
    }

    /// Every language mark the site's file names use.
    pub fn language_marks(&self) -> Vec<String> {
        self.content_paths
            .iter()
            .flat_map(|c| c.langs.values())
            .map(|p| p.suffix.clone())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// The file name of `filename`'s counterpart in another language: the same post, its
    /// mark changed (`post.md` ↔ `post.fr.md`); the same name where languages have folders.
    pub fn counterpart(&self, filename: &str, from: &str, to: &str) -> String {
        let (from, to) = (self.language_mark(from), self.language_mark(to));
        if from.is_empty() && to.is_empty() {
            return filename.to_string();
        }
        let (stem, extension) = filename.rsplit_once('.').unwrap_or((filename, "md"));
        let base = if from.is_empty() {
            stem
        } else {
            stem.strip_suffix(&format!(".{from}")).unwrap_or(stem)
        };
        if to.is_empty() {
            format!("{base}.{extension}")
        } else {
            format!("{base}.{to}.{extension}")
        }
    }

    /// The site's languages, in order; `en` when none is known yet.
    pub fn languages(&self) -> Vec<String> {
        if let Some(cp) = &self.content_paths {
            if !cp.langs.is_empty() {
                return cp.langs.keys().cloned().collect();
            }
        }
        vec!["en".to_string()]
    }

    /// Renames a language, keeping its folders: for a one-language site detected with the
    /// wrong code.
    pub fn rename_language(&mut self, from: &str, to: &str) {
        let to = to.trim().to_lowercase();
        if to.is_empty() || from == to {
            return;
        }
        if let Some(cp) = &mut self.content_paths {
            if let Some(paths) = cp.langs.remove(from) {
                cp.langs.insert(to, paths);
            }
        }
    }

    /// Where images go, and the address they are linked at: as configured, or where the
    /// site's framework serves static files from.
    pub fn images(&self, project_path: &str) -> (String, String) {
        if !self.images_dir.is_empty() {
            let url = if self.images_url.is_empty() {
                format!("/{}", self.images_dir.trim_matches('/'))
            } else {
                self.images_url.clone()
            };
            return (self.images_dir.clone(), url);
        }
        let public = Path::new(project_path).join("public").is_dir();
        let (dir, url) = match self.preset {
            Some(Preset::Hugo) | Some(Preset::Zola) => ("static/images", "/images"),
            Some(Preset::Jekyll) => ("assets/images", "/assets/images"),
            Some(Preset::Docusaurus) => ("static/img", "/img"),
            Some(Preset::Astro) | Some(Preset::Next) => ("public/images", "/images"),
            _ if public => ("public/images/blog", "/images/blog"),
            _ => ("images", "/images"),
        };
        (dir.to_string(), url.to_string())
    }

    /// Where a post lives on the site, by its `slug` field when it has one.
    pub fn address(&self, lang: &str, filename: &str, content: &str) -> String {
        match frontmatter::text(&frontmatter::fields(content), "slug") {
            Some(slug) if !slug.trim().is_empty() => {
                self.post_url(lang, &format!("{}.md", slug.trim()))
            }
            _ => self.post_url(lang, filename),
        }
    }

    /// Where a post lives on the site.
    pub fn post_url(&self, lang: &str, filename: &str) -> String {
        let pattern = if self.post_url.is_empty() {
            "{site_url}/{lang}/blog/{slug}"
        } else {
            &self.post_url
        };
        let today = chrono::Local::now();
        pattern
            .replace("{site_url}", self.site_url.trim_end_matches('/'))
            .replace("{lang}", lang)
            .replace("{slug}", &slug_of(filename))
            .replace("{year}", &today.format("%Y").to_string())
            .replace("{month}", &today.format("%m").to_string())
            .replace("{day}", &today.format("%d").to_string())
    }
}

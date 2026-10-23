# Changelog

What changed in each release of **PublishKit**, the desktop app for writing,
queuing and publishing blog posts.

The public version of this page — with the download for each release — lives at
<https://mayorana.ch/en/apps/publishkit/releases>. It is generated from this
file by `scripts/changelog_to_json.py`, so this file is the only place a
release note is written.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each heading is dated on the day its tag was pushed. Releases that carried only
build or packaging work say so rather than being hidden: the version numbers a
user sees in the update prompt should all be accounted for.

## [0.1.23] - 2026-10-23

### Changed

- The usage-statistics notice now looks the same as in GitAgent: a blue bar
  along the bottom of the window.
- Every action button is now 30px tall, borders included, like the
  regular buttons beside it.

### Added

- The model is now shown at the top of the window with its region's flag.
  Click it to switch between Europe, China and the United States without
  opening Settings.

## [0.1.22] - 2026-10-23

### Changed

- Dialogs now have a title bar with a × to close them, and text fields have
  the same size and typeface as in GitAgent.
- The buttons along the top of the window now all have the same height,
  borders included, and the light/dark switch matches them.

### Fixed

- On Windows, post paths in replies to Claude now use `\` throughout,
  instead of mixing it with the `/` from publishkit.yaml.

## [0.1.21] - 2026-10-23

### Changed

- Buttons now have the same size, rounded corners and typeface as in
  GitAgent, so they look alike across the Mayorana apps.

## [0.1.20] - 2026-10-23

### Added

- API requests now include a header specifying the task kind, allowing api0 to
  route writing tasks to the appropriate region-specific model. This ensures
  that the writing process is optimized for the selected region and leverages
  the AI credits shared across Mayorana apps on the same computer.

## [0.1.19] - 2026-10-22

### Removed

- DeepSeek and Claude with your own key are gone from Settings: the model is
  api0, in Europe, China or the United States. A config saved with one of
  them opens on api0 · Europe, with its sites and accounts kept.

## [0.1.18] - 2026-10-22

### Added

- Settings offers api0 as the model, in one of three places: Europe, China
  or the United States, each one click with its flag. It runs on the AI
  credits of the licence every Mayorana app on this computer shares, so a
  licence pasted in GitAgent or JobScout works here with no API key to set
  up. Each region shows about how many credits a post costs. DeepSeek and
  Claude with your own key are still one click away.

## [0.1.17] - 2026-10-22

### Added

- PublishKit now works with sites built with Hugo, Jekyll, Astro, Eleventy, Zola,
  Docusaurus or Next.js, or with any folder of markdown posts, not just the
  `content/{lang}/queue` and `content/{lang}/blog` layout. Open the site's
  folder and PublishKit finds its posts and how it marks a draft (`draft:
  true`, or Jekyll's `_drafts` folder). New posts are written the way that site
  writes them.
- Any number of languages: every language folder of the site gets its own tab,
  and the model writes in that language. A site in one language shows no tabs.
- Settings has a **This site** section: what the site is built with, its name,
  address, author, default tags, writing brief, the address of a post (used in
  the social teasers), and a command to run after each publish.
- Posts written in TOML (`+++`) front matter are read and published.
- Editing a post shows its title, description, date and tags as fields of
  their own above the text, with its word count and reading time. Only the
  fields you change are rewritten in the file.
- Posts save themselves a moment after you stop typing, and the post you were
  on is saved when you open another.
- Add images to a post with **Image…** or by dropping them on the text: they
  are copied into the site's images folder (set in Settings) and linked where
  the cursor is.
- Translate a post into the site's other languages with the **→ FR** (and so
  on) buttons in the editor: the translation is saved as a draft under the
  same file name in that language.
- Search the post list by words in the title, description, tags or text, and
  filter it by tag.
- A **Board** tab shows drafts by stage — ideas, drafts, in review — next to
  what is scheduled and what went out last. Add an idea with just a title and
  move cards along as the writing goes.
- A **Calendar** tab shows each month's published posts on their date and
  scheduled drafts on the day they go out. Click any post to edit it.
- Schedule a draft: pick its stage and a **Publish on** date and time in the
  editor, and PublishKit publishes it then while its window is open. For when
  it isn't, `publishkit publish-due` does the same from cron or launchd.
- Claude can set a draft's stage and publishing time too, and sees them when
  it lists posts.
- Checks before publishing, listed in the editor as you write. Problems — no
  title or text, a TODO or lorem ipsum left in, an image that isn't in the
  site — hold Publish back until you fix them or publish anyway, and a
  scheduled draft with problems is unscheduled rather than sent out. Warnings
  point at titles and descriptions too long for search results, missing
  descriptions or tags, headings that skip a level and images without alt
  text. **Check links** reports links to other sites that no longer work.
- A **Deploy** button runs the command that puts your site online, set in
  Settings, and can run it after each publish too.
- Send a published post to dev.to, Hashnode, a WordPress or a Ghost blog from
  its editor, with your site's address as the original so search engines
  credit it. Copies go as drafts unless you choose otherwise, and sending
  again updates the same copy. Claude can cross-post too, when you ask.
- Posts kept as page bundles — a folder per post, holding `index.md` and its
  images, as Hugo and Astro sites often do — are listed and edited like any
  other, and publishing or deleting one moves or removes its whole folder.
  On such a site new posts are bundles too, and images you add go into the
  post's own folder.
- Widen the post list by dragging its right edge; double-click the edge to
  put it back. As it widens, titles grow, the Publish buttons move beside
  them and each post shows a line or two of its description.
- The preview shows the post's own images — those in the site's images
  folder or beside the post — instead of empty boxes.
- A formatting bar above the text — bold, italic, link, headings, list,
  quote, code — with ⌘B, ⌘I and ⌘K, and images can be pasted straight from
  the clipboard.
- Post a post's short teaser on Bluesky or Mastodon from its preview, with
  your account set up in Settings.
- Hugo and Zola sites that mark the language in the file name (`post.fr.md`)
  get a tab per language, and translating a post writes its counterpart
  under the right name.
- Switch between your sites from the menu at the top left, which remembers
  every site you open; the start screen lists them as well. Scheduled posts
  go out for all of them while PublishKit is open, and
  `publishkit publish-due --all` does the same from cron or launchd.

### Changed

- Publishing only changes the lines it needs (status, draft mark, date) and
  keeps everything else in the post's front matter as it was, comments
  included.
- The command run after publishing is shown in Settings and can be changed or
  removed. If it fails, you now see its error instead of nothing; the post
  stays published.
- A site opened for the first time gets neutral settings named after its
  folder, instead of another site's name, tags and writing brief.
- Posts are listed newest first.
- The language picker in the new-post form is no longer squeezed.
- The window and top bar are now called PublishKit, and the top bar shows what
  the site is built with.
- The site menu replaces the **Change folder** button.

### Fixed

- The new-post form starts empty, instead of showing the title and summary
  of the last post you opened.
- The posts of the site you had open are listed as soon as PublishKit starts,
  instead of only after you press refresh.
- Saving a published post in the editor saves it where it is, instead of
  creating a copy among the drafts.
- A new post with the same title as an existing draft gets a file of its own
  instead of replacing it.
- Descriptions written over several lines in the front matter show correctly.

## [0.1.16] - 2026-10-20

### Added

- A Feedback button in the top bar opens the contact form with PublishKit and
  your version already filled in, so an idea or a problem reaches us in two
  clicks.
### Changed

- The Windows installer no longer asks for administrator rights. PublishKit now
  installs for your account only, in your user folder, and Node.js is set up
  the same way, so you can install on a work PC without an admin password. The
  installer still offers an install for all users, and an existing all-users
  install is updated in place.

## [0.1.15] - 2026-10-19

### Changed

- The app icon has been updated to a new design, featuring a page with a folded
  corner and text lines, along with a paper plane leaving it. The new icon gives
  a modern and cohesive look to the application.

## [0.1.14] - 2026-10-19

### Changed

- Packaging only — no user-visible change.

## [0.1.13] - 2026-10-18

### Added

- Use PublishKit from Claude. The new **MCP** button in the top bar shows what
  to paste into Claude Desktop, or the one line to run for Claude Code,
  already pointing at where PublishKit is installed. Claude can then read your
  site's posts, write and queue drafts in each language (or have PublishKit's
  own model write them), edit drafts, set their LinkedIn and X teasers, queue
  LinkedIn posts, and publish or delete drafts when you ask. The lists in the
  window follow within a couple of seconds.

## [0.1.12] - 2026-10-16

### Added

- PublishKit now includes an icon in the Linux and Windows installers, making it
  easier to identify the application in your system's application launcher and
  taskbar.

## [0.1.11] - 2026-10-14

### Changed

- Blog Toolkit is now called PublishKit. Your API keys and settings carry over.
  A project's `blog-toolkit.yaml` is still read and kept where it is; new
  projects get a `publishkit.yaml`. On macOS the update installs as
  PublishKit.app; once it runs, you can delete Blog Toolkit.app from
  Applications.

## [0.1.10] - 2026-10-07

### Changed

- The installer now includes a license agreement, ensuring users are aware of
  the terms before installing PublishKit.

## [0.1.9] - 2026-10-06

### Added

- PublishKit now shares anonymous usage statistics: whether it is installed and
  opened, its version and your operating system. It is on by default; a note at
  the bottom of the window tells you once, and nothing is sent before you have
  seen it. **Turn off** there stops it for good and deletes anything not yet
  sent. Your files, data and accounts are never part of it. It also stays off if
  `DO_NOT_TRACK`, `DISABLE_UPDATE_CHECK` or `MAYORANA_NO_TELEMETRY` is set.

## [0.1.8] - 2026-10-02

### Changed

- Packaging only — no user-visible change.

## [0.1.7] - 2026-08-28

### Changed

- The macOS download is a signed and notarized `.dmg` with a regular app
  bundle, which opens with a normal double-click. It used to be a bare program
  that macOS refused to open because the developer could not be verified. The
  disk image also carries `setup-mac.sh`, which installs Node.js for
  publishing if you need it.

## [0.1.6] - 2026-08-28

### Changed

- Packaging only — no user-visible change.

## [0.1.5] - 2026-08-27

### Added

- A LinkedIn tab: generate a LinkedIn post from a product or topic and the
  points you want to make, in French or English, keep it in a queue, and mark
  it as posted once you have published it on LinkedIn.
- Each generated blog post comes with a LinkedIn and an X teaser, shown under
  the post with a copy button.
- Per-project settings in a `blog-toolkit.yaml` file at the project root, so
  several sites can each have their own prompts and author. The content
  folders for each language are detected the first time a project is opened.
- A light and dark theme toggle, following the system theme at startup.

### Changed

- Downloads now come from mayorana.ch instead of GitHub.
- PublishKit is source-available under the PolyForm Noncommercial licence:
  free for personal, educational and noncommercial use.

## [0.1.4] - 2026-05-22

### Changed

- Packaging only — no user-visible change.

## [0.1.3] - 2026-05-22

### Changed

- Blog Manager is now called Blog Toolkit. Settings are stored under the new
  name, so API keys saved in an earlier version have to be entered again.

## [0.1.2] - 2026-05-22

### Changed

- Packaging only — no user-visible change.

## [0.1.1] - 2026-05-22

### Added

- First release. Open a blog project folder and see its queued and published
  posts per language, with a rendered preview of the selected post.
- Generate a post from a title and a short summary with DeepSeek or Claude,
  and save it to the queue as a draft.
- Edit a queued post's markdown, then publish it to the site or delete it.

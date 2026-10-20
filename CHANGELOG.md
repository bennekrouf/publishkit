# Changelog

What changed in each release of **PublishKit**, the desktop app for writing,
queuing and publishing blog posts with DeepSeek or Claude.

The public version of this page — with the download for each release — lives at
<https://mayorana.ch/en/apps/publishkit/releases>. It is generated from this
file by `scripts/changelog_to_json.py`, so this file is the only place a
release note is written.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each heading is dated on the day its tag was pushed. Releases that carried only
build or packaging work say so rather than being hidden: the version numbers a
user sees in the update prompt should all be accounted for.

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

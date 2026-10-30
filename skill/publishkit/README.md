# PublishKit skill

A skill for AI agents that use **PublishKit**, the desktop app that keeps your website's blog as markdown drafts per language, with a publishing queue, a schedule, and LinkedIn and X posts. The
skill tells the agent how to use PublishKit's tools well: which to call first, what
needs your yes, sensible defaults, and how to report back.

## What you need

- **PublishKit**, installed from <https://mayorana.ch/en/apps/publishkit> (macOS,
  Windows or Linux). This skill does not include the app.
- A desktop agent connected to PublishKit's MCP server: Claude Desktop, Claude
  Code, Codex CLI or Cursor. `references/connect.md` has the steps. Web-only
  agents such as ChatGPT can't reach an app on your computer.

## What you can ask for

- Draft posts in your site's voice, or have PublishKit's model write them
- Translate a post into your site's other languages
- Plan a content calendar from what is already published
- Save LinkedIn and X posts exactly as you wrote them
- Publish, schedule and re-date posts

## What waits for your yes

- Publishing a post (it runs your site's publish command)
- Scheduling a post to go live
- Deleting a draft

## Limits

PublishKit never posts to LinkedIn or X itself: those drafts are reminders on the calendar.

## Install

- **Claude Code**: unzip into `~/.claude/skills/` so the folder is
  `~/.claude/skills/publishkit/`.
- **claude.ai or Claude Desktop**: upload the zip under Skills in Settings.
- **Other agents**: put the `publishkit` folder where the agent reads skills.

## Help

Questions, licences and bug reports: <https://mayorana.ch/en/contact>.

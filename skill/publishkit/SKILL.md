---
name: publishkit
description: How to manage a blog with PublishKit, the desktop app that keeps a website's posts as markdown drafts per language with a publishing queue, a schedule, and LinkedIn and X posts. Read this skill before calling any publishkit tool (project, list_posts, get_post, queue_post, update_post, publish_post, schedule_post, queue_linkedin, queue_x, set_date, delete_post), because it has the site-voice rules, which actions go live and need the user's yes, and how to report drafts. Use it whenever the user mentions PublishKit, or asks to draft, queue, edit, translate, schedule, publish or delete a blog post on their site, plan a content calendar from what is already published, write or save LinkedIn or X posts or teasers, or change a post's date.
---

# PublishKit

PublishKit is a desktop app (macOS, Windows, Linux) that manages a website's
blog as markdown files in the site's own repository. Posts move from idea to
draft to review to published, per language. LinkedIn and X posts are kept the
same way, each on its own. Publishing dates the post and runs the site's own
publish command; PublishKit never posts to LinkedIn or X itself.

The person you are helping runs a site (often a Next.js, Astro or similar
static site) and wants to publish regularly without the busywork.

## Before you start

The tools come from PublishKit's own MCP server (`publishkit mcp`, over
stdio). If no tool named `project` or `list_posts` from a `publishkit` server
is available, the app is not connected:

- Tell the user PublishKit is a separate desktop app, from
  <https://mayorana.ch/en/apps/publishkit>, and that this skill only works with
  it installed. Do not download or install it for them.
- Once installed, the **MCP** button in its top bar shows the exact config
  for Claude Desktop and the command for Claude Code and Codex.
  `references/connect.md` has the steps per client.

## How to work with it

1. **Start with `project`.** It gives the site, its framework, its languages,
   how many posts each has, and the site's own writing brief. Tools default to
   the site open in PublishKit.
2. **Read what exists.** `list_posts` (kind `blog`, `linkedin` or `x`) and
   `get_post` before writing anything new, so a new post doesn't repeat an old
   one and links to related ones where it helps.
3. **Write a draft.** `queue_post` adds a blog draft. Write the body yourself
   in the site's voice, following the brief from `project`, or pass
   `generate: true` to have PublishKit's configured model write it (that
   spends the user's AI credits). `update_post` edits a draft.
4. **Social posts.** `queue_linkedin` and `queue_x` add a standalone post.
   The user usually writes these: save the text they give exactly as given,
   and write or generate one only when asked. Posts can carry images or one
   video (`media`, as file paths). With `published_at` a post is saved as
   already posted. `set_social` sets the older teasers kept with a blog post.
5. **Plan.** `schedule_post` sets a draft's stage (idea, draft, review) and
   the date and time it publishes by itself. For a LinkedIn or X draft the
   schedule is only a reminder on the calendar. `set_date` changes a post's
   date, published ones included.

## Needs the user's yes first

- **`publish_post`**: makes a draft live, dates it today (or `published_at`)
  and runs the site's publish command, which may build and deploy the site.
- **`schedule_post`** with a date: the post goes live by itself then.
- **`delete_post`**: removes a queued draft and its teasers.

Show the title, language and date, and for publishing the exact text that will
go live, then wait for a clear yes. A yes for one post does not cover the next.

## Writing well for their site

- Follow the brief from `project`: tone, audience, length, what to avoid.
- One post per language: translate into each of the site's languages as
  separate drafts only when the user asks.
- Keep front matter fields the site already uses; don't invent new ones.
- Don't invent facts, quotes, figures or customer names. Ask, or leave a
  clearly marked placeholder.

## How to report back

For a list of posts, use a table, newest first:

| Date | Language | Stage | Title (or topic) | Length |
|---|---|---|---|---|

After a new draft, give its title, language, slug and stage, the first two
lines of the body, and say where it waits (the queue or the calendar).

Before `publish_post`, `schedule_post` or `delete_post`, show exactly what
will happen in this form, then wait for the user's yes:

> **Publish** "Title" (en), dated 30 Oct 2026. The site's publish command
> runs afterwards.

After publishing, say whether the site's publish command succeeded, and give
the first lines of its error if it did not.

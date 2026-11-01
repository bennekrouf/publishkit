---
name: publishkit
description: How to manage a blog with PublishKit, the desktop app that keeps a website's posts as markdown drafts per language with a publishing queue, a schedule, and LinkedIn, X, Instagram and TikTok posts (some folders hold social posts only). Read this skill before calling any publishkit tool (project, list_posts, get_post, queue_post, update_post, publish_post, schedule_post, queue_linkedin, queue_x, queue_instagram, queue_tiktok, repurpose, post_now, set_date, delete_post, and the shared media library tools list_media, calendar, plan_media, fulfil_plan, set_media_status), because it has the site-voice rules, which actions go live and need the user's yes, and how to report drafts. Use it whenever the user mentions PublishKit, or asks to draft, queue, edit, translate, schedule, publish or delete a blog post on their site, plan a content calendar from what is already published, write or save LinkedIn, X, Instagram or TikTok posts or teasers, change a post's date, or repurpose a finished video or audio recording (PolishCast, TrackCut) into clips, carousel slides, social posts and a blog draft.
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
4. **Social posts.** `queue_linkedin`, `queue_x`, `queue_instagram` and
   `queue_tiktok` add a standalone post. The user usually writes these: save
   the text they give exactly as given, and write or generate one only when
   asked. Posts can carry images or one video (`media`, as file paths); an
   Instagram carousel mixes both. An Instagram or TikTok post is a caption
   under its photos or video: say when one has no media yet, and don't paste
   links into it, where they can't be clicked (the link goes in the bio).
   Instagram takes at most 5 hashtags in a post or reel. With
   `published_at` a post is saved as already posted. `set_social` sets the
   older teasers kept with a blog post. When `project` says `blog: false`,
   the folder holds social posts only: don't offer blog drafts.
5. **Repurpose a recording.** `repurpose` turns one finished PolishCast
   video or TrackCut audio file and its transcript into drafts in one call
   to PublishKit's model (it spends the user's AI credits): short clips as
   TikTok drafts with their start and end, an Instagram carousel with its
   slides, a LinkedIn and an X post, and a blog draft or outline. The
   transcript is required: pass its text or file, or it is found beside the
   recording by the same name (`talk.srt` for `talk.mp4`); subtitles give
   clips their times, plain text only the passage to cut. PublishKit doesn't
   cut the clips or draw the slides: say so, and that every draft can be
   edited or deleted before posting. When the user has no transcript, ask
   for one rather than making times up.
   To give a TikTok draft its video, call PolishCast's `export_clip` with
   the draft's `repurposed_from` as `video` and its clip start and end, then
   `update_post` with the clip as `media`.
6. **Plan.** `schedule_post` sets a draft's stage (idea, draft, review) and
   the date and time it publishes by itself. For a network's draft the
   schedule is only a reminder on the calendar. `set_date` changes a post's
   date, published ones included.
7. **The media library.** `list_media`, `get_media` and `calendar` read the
   library PolishCast, TrackCut and PublishKit share: every take, recording
   and post with its status, files and the drafts it went into. `plan_media`
   puts an idea (a video to record, a post to write) on the calendar with its
   outline as notes; `fulfil_plan` hands it to what was made for it.
   `set_media_status`, `set_media_tags` and `set_media_notes` change an item.
   When the user says a social post is up, `publish_post` with its `url`
   keeps the address.

## From an idea to LinkedIn and TikTok

1. `plan_media` the video, with its script as notes; the user agrees the day.
2. PolishCast: `start_recording`, `stop_recording`, the edits, `transcribe`,
   `export` with `captions: files`.
3. `fulfil_plan` with the take (`list_media` gives its id).
4. `repurpose` the export (asking first: it spends credits), then read the
   drafts back and fix what the user asks.
5. For each TikTok draft: PolishCast `export_clip`, then `update_post` with
   it. The LinkedIn draft gets the full export.
6. `schedule_post` each draft, with the user's yes. PublishKit doesn't post
   to LinkedIn or TikTok itself; at the planned time it reminds the user,
   and the post goes up through Claude in Chrome (below) or by hand with the
   draft's Post now button.
7. `publish_post` with the post's `url` records it, and the take shows as
   posted in the library.

## Posting through Claude in Chrome

When the user asks to post a queued LinkedIn, X, Instagram or TikTok draft
and Claude in Chrome is connected (its tools are `mcp__claude-in-chrome__*`;
read its skill first):

1. `post_now` gives the network's page (`compose_url`), the text exactly as
   saved, the files to attach and anything `missing`. Fix what is missing
   first (attach the clip, shorten the text), never post around it.
2. Open `compose_url` in a new tab of the user's Chrome, where they are
   signed in. Never sign in for them: if a login page shows, ask them to.
3. Paste the text as saved, without rewording it, and attach every file in
   `media` with the file upload tool, in order. On TikTok, wait for the
   video to finish processing.
4. Stop before the network's Post button. Show the user what is ready
   (network, first line of the text, files) and ask for a yes for this
   post. A yes covers this one post only, never the next.
5. After their yes, click Post, wait for the post to appear, and read its
   address (open the post or the profile's latest post if the page doesn't
   show it).
6. `publish_post` with `kind`, `filename` and that `url`. Report the address.

If anything on the page differs from the draft (an account other than the
user's, a dialog asking for permissions, a captcha), stop and tell the
user; never accept terms or permissions for them.

## Needs the user's yes first

- **`publish_post`**: makes a draft live, dates it today (or `published_at`)
  and runs the site's publish command, which may build and deploy the site.
- **`schedule_post`** with a date: the post goes live by itself then.
- **`delete_post`**: removes a queued draft and its teasers.
- **Clicking Post on LinkedIn, X, Instagram or TikTok** in Chrome: it goes
  out under the user's name at once.

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

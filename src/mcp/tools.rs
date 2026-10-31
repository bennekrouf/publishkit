//! The tools PublishKit offers an MCP client, as the client sees them.

use super::*;

pub(super) fn tools() -> Value {
    let project = json!({ "type": "string", "description": "Absolute path of the site's folder. Defaults to the one open in PublishKit." });
    let lang = json!({ "type": "string", "description": "Language code, e.g. fr or en (project lists them)." });
    let filename = json!({ "type": "string", "description": "The post's file name as list_posts gives it: my-post.md, my-post.mdx, or my-post/index.md for a page bundle." });
    let kind = json!({ "type": "string", "enum": ["blog", "linkedin", "x", "instagram", "tiktok"], "description": "Default blog." });
    let place = json!({ "type": "string", "enum": ["queue", "published"], "description": "Default queue." });
    let media = json!({ "type": "array", "items": { "type": "string" }, "description": "A LinkedIn, X, \
        Instagram or TikTok post's images (png, jpg, gif; up to 20 on LinkedIn and Instagram, 4 on X, 35 on \
        TikTok) or its one video (mp4, mov, webm, mkv, avi); an Instagram carousel mixes photos and videos. In order: absolute paths of files, copied beside the post and listed in its front matter, or names of \
        files it has already. Replaces the post's media; [] takes them off." });
    let published_at = json!({ "type": "string", "description": "When it actually went out, for a post \
        posted earlier and saved here after: 2026-10-16, or with the local time, 2026-10-16T09:00. Default now." });
    json!([
        {
            "name": "project",
            "description": "The site: its name, URL, author, default tags, its writing brief (the system prompt \
                            PublishKit writes with), the framework it is built with, its languages, and how many \
                            drafts and published posts each has, blog, LinkedIn, X, Instagram and TikTok. Also the model PublishKit \
                            generates with, and the other sites PublishKit knows (pass one's folder as project).",
            "inputSchema": { "type": "object", "properties": { "project": project } },
        },
        {
            "name": "list_posts",
            "description": "Posts with their title (or a network post's topic), date, status and length: every \
                            draft with its description, and the newest published posts a page at a time.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": { "type": "string", "description": "One language; default all." },
                    "kind": kind,
                    "where": { "type": "string", "enum": ["queue", "published", "all"], "description": "Default all." },
                    "limit": { "type": "integer", "description": "Published posts per language; default 10." },
                    "offset": { "type": "integer", "description": "Published posts to skip, newest first; default 0." },
                },
            },
        },
        {
            "name": "get_post",
            "description": "One post in full: its front matter, its markdown, and its social teasers.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "lang": lang, "filename": filename, "kind": kind, "where": place },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "queue_post",
            "description": "Adds a blog draft to a language's queue, with the site's author and default tags. When \
                            project says title_heading, the file starts with the title as a heading, so don't repeat \
                            it in the body. Give body, or \
                            generate: true to have PublishKit's model write it from the title and summary. social \
                            adds the teasers; generate_social has PublishKit's model write them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": lang,
                    "title": { "type": "string" },
                    "summary": { "type": "string", "description": "One or two sentences: the post's description." },
                    "body": { "type": "string", "description": "Markdown: intro, ## sections, conclusion." },
                    "generate": { "type": "boolean" },
                    "social": {
                        "type": "object",
                        "properties": { "linkedin": { "type": "string" }, "twitter": { "type": "string" } },
                    },
                    "generate_social": { "type": "boolean" },
                },
                "required": ["lang", "title", "summary"],
            },
        },
        {
            "name": "update_post",
            "description": "Changes a draft (blog, LinkedIn, X, Instagram or TikTok): body replaces what follows \
                            the front matter, content replaces the whole file (front matter included), media sets \
                            a network post's images or video. Published posts aren't edited here, except a posted \
                            network post's media (where: published), so a post is recorded with what it showed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind, "where": place,
                    "body": { "type": "string" },
                    "content": { "type": "string" },
                    "media": media,
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "set_social",
            "description": "Sets the LinkedIn and X teasers kept with a blog post, or has PublishKit's model \
                            write them (generate: true), with the post's URL on the site. For a post of its own on \
                            a network, on its list and calendar, use queue_linkedin, queue_x, queue_instagram or \
                            queue_tiktok.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "where": place,
                    "linkedin": { "type": "string" },
                    "twitter": { "type": "string", "description": "At most 280 characters, the URL included." },
                    "generate": { "type": "boolean" },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "publish_post",
            "description": "Publishes a draft: moves it into the blog (or a network's posted folder), or clears \
                            its draft mark where drafts sit with published posts, dates it today (or published_at), \
                            and for a blog post runs the site's publish command, if it has one. A LinkedIn, X, \
                            Instagram or TikTok post is only recorded as posted: PublishKit doesn't post to them. A blog post whose checks \
                            find problems (no title or text, TODO left in, a missing image) is refused with them; \
                            get_post lists its checks. Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind,
                    "published_at": published_at,
                    "force": { "type": "boolean", "description": "Publish a blog post although its checks found problems. Only when the user says so." },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "schedule_post",
            "description": "Sets a blog draft's stage on PublishKit's board (idea, draft or review) and when it is \
                            published by itself: publish_at, local time like 2026-11-02T09:00, or an empty string \
                            to unschedule. PublishKit publishes it then while its window is open, or when \
                            `publishkit publish-due` runs. With a network's kind (linkedin, x, instagram, tiktok), publish_at only plans the draft \
                            on the calendar: PublishKit can't post to those networks, so the user posts it and it \
                            is then recorded with publish_post. Ask the user before scheduling.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind,
                    "stage": { "type": "string", "enum": ["idea", "draft", "review"] },
                    "publish_at": { "type": "string" },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "crosspost_post",
            "description": "Sends a published blog post to dev.to, Hashnode, a WordPress or a Ghost blog set up in \
                            PublishKit's Settings, with the site's address as canonical — or updates the copy sent \
                            before. Copies go as drafts there unless the user turned that off. Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename,
                    "target": { "type": "string", "enum": ["devto", "hashnode", "wordpress", "ghost"] },
                },
                "required": ["lang", "filename", "target"],
            },
        },
        {
            "name": "delete_post",
            "description": "Deletes a queued draft and its teasers. Published posts aren't deleted here. Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "lang": lang, "filename": filename, "kind": kind },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "set_date",
            "description": "Sets the date, and the time if given, of a post: published (default) or queued, blog, \
                            LinkedIn, X, Instagram or TikTok. For a post that went out at another time than PublishKit recorded. The \
                            calendar shows it there. A blog post's file name keeps its date prefix, if it has one. \
                            Ask the user first for a published blog post: the site shows the date.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind,
                    "where": { "type": "string", "enum": ["queue", "published"], "description": "Default published." },
                    "date": { "type": "string", "description": "2026-10-16, or with the local time, 2026-10-16T09:00." },
                },
                "required": ["lang", "filename", "date"],
            },
        },
        social_tool(Channel::LinkedIn, &project, &lang, &media),
        social_tool(Channel::X, &project, &lang, &media),
        social_tool(Channel::Instagram, &project, &lang, &media),
        social_tool(Channel::TikTok, &project, &lang, &media),
        {
            "name": "repurpose",
            "description": "Turns one finished recording (a PolishCast video, a TrackCut audio file) and its \
                            transcript into drafts, in one call to PublishKit's model: 3-8 short clips as TikTok \
                            drafts (Instagram reels when TikTok is hidden), each with its start and end in the \
                            recording, its on-screen hook and caption; an Instagram carousel draft with its slides \
                            (quote cards) and caption; a LinkedIn and an X post; and a blog draft or outline in \
                            the site's voice. Nothing is cut or drawn: the user cuts the clips and makes the \
                            slides, and edits or deletes any draft before posting. The transcript is required: \
                            subtitles (.srt, .vtt) give clips their times, plain text only the passage to cut. \
                            Without transcript or transcript_file, one beside the recording by the same name \
                            (talk.srt for talk.mp4) is used. Texts are kept within each network's limits.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": lang,
                    "media": { "type": "string", "description": "Absolute path of the video or audio file." },
                    "transcript": { "type": "string", "description": "The transcript's text: SRT or WebVTT subtitles, or plain text." },
                    "transcript_file": { "type": "string", "description": "Absolute path of a .srt, .vtt or .txt transcript." },
                    "title": { "type": "string", "description": "What the recording is about, to steer the writing." },
                    "clips": { "type": "integer", "minimum": 3, "maximum": 8, "description": "How many clips. Default 5." },
                    "blog": { "type": "string", "enum": ["draft", "outline", "none"], "description": "Default draft; none on a site without a blog." },
                    "networks": { "type": "array", "items": { "type": "string", "enum": ["linkedin", "x", "instagram", "tiktok"] }, "description": "Default: the site's networks." },
                },
                "required": ["lang", "media"],
            },
        },
    ])
}

/// `queue_linkedin`, `queue_x`, `queue_instagram` and `queue_tiktok`.
fn social_tool(channel: Channel, project: &Value, lang: &Value, media: &Value) -> Value {
    let label = channel.label();
    let limit = match channel {
        Channel::X => " X takes 280 characters (up to 25,000 with X Premium).",
        Channel::LinkedIn => " LinkedIn takes 3,000 characters.",
        Channel::Instagram => " Instagram takes 2,200 characters and 5 hashtags in a caption.",
        Channel::TikTok => " TikTok's app takes 4,000 characters in a caption.",
    };
    // Instagram and TikTok show an address as plain text, and post nothing without a photo or video.
    let link_rule = if channel.links_clickable() {
        format!(
            "A link goes in the body as the bare address, on a line of its own: {label} makes it \
                 clickable, and shows markdown links as they are typed."
        )
    } else {
        format!("The body is the caption under its photos or video: {label} can't show a post without \
                 media, so attach it with media (or the user adds it in PublishKit). Links in a {label} \
                 caption can't be clicked: say the link is in the bio rather than pasting it.")
    };
    let link_field = if channel.links_clickable() {
        format!("For generate: an address the post links to. It is put in as it is, on a line of its own \
                 near the end: {label} makes it clickable, and shows markdown links as plain text.")
    } else {
        format!(
            "For generate: the page the post is about. {label} can't link from a caption, so it \
                 isn't put in: the caption points to the bio."
        )
    };
    json!({
        "name": format!("queue_{}", channel.key()),
        "description": format!("Adds a {label} post to a language's queue, on its own, not tied to a blog post. \
                        Give body, the text as the user wrote it or as you wrote it with them; or generate: true \
                        with a topic, an angle and, if it links somewhere, a link to have PublishKit's model \
                        write it. {link_rule} topic names it in the list; without one, its first line does. For a post already on {label}, give \
                        published_at and it is saved as posted on that date. media attaches its images or \
                        video.{limit}"),
        "inputSchema": {
            "type": "object",
            "properties": {
                "project": project,
                "lang": lang,
                "topic": { "type": "string" },
                "body": { "type": "string" },
                "angle": { "type": "string", "description": "Key points, for generate." },
                "link": { "type": "string", "description": link_field },
                "generate": { "type": "boolean" },
                "published_at": { "type": "string", "description": format!("Already posted on {label}: \
                    when, 2026-10-16 or 2026-10-16T09:00. The post is saved straight among the posted ones.") },
                "media": media,
            },
            "required": ["lang"],
        },
    })
}

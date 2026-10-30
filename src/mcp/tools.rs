//! The tools PublishKit offers an MCP client, as the client sees them.

use super::*;

pub(super) fn tools() -> Value {
    let project = json!({ "type": "string", "description": "Absolute path of the site's folder. Defaults to the one open in PublishKit." });
    let lang = json!({ "type": "string", "description": "Language code, e.g. fr or en (project lists them)." });
    let filename = json!({ "type": "string", "description": "The post's file name as list_posts gives it: my-post.md, my-post.mdx, or my-post/index.md for a page bundle." });
    let kind = json!({ "type": "string", "enum": ["blog", "linkedin", "x"], "description": "Default blog." });
    let place = json!({ "type": "string", "enum": ["queue", "published"], "description": "Default queue." });
    let media = json!({ "type": "array", "items": { "type": "string" }, "description": "A LinkedIn or X \
        post's images (png, jpg, gif; up to 20 on LinkedIn, 4 on X) or its one video (mp4, mov, webm, mkv, avi), \
        in order: absolute paths of files, copied beside the post and listed in its front matter, or names of \
        files it has already. Replaces the post's media; [] takes them off." });
    let published_at = json!({ "type": "string", "description": "When it actually went out, for a post \
        posted earlier and saved here after: 2026-10-16, or with the local time, 2026-10-16T09:00. Default now." });
    json!([
        {
            "name": "project",
            "description": "The site: its name, URL, author, default tags, its writing brief (the system prompt \
                            PublishKit writes with), the framework it is built with, its languages, and how many \
                            drafts and published posts each has, blog, LinkedIn and X. Also the model PublishKit \
                            generates with, and the other sites PublishKit knows (pass one's folder as project).",
            "inputSchema": { "type": "object", "properties": { "project": project } },
        },
        {
            "name": "list_posts",
            "description": "Posts with their title (or a LinkedIn or X post's topic), description, date, status and length.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": { "type": "string", "description": "One language; default all." },
                    "kind": kind,
                    "where": { "type": "string", "enum": ["queue", "published", "all"], "description": "Default all." },
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
            "description": "Changes a draft (blog, LinkedIn or X): body replaces what follows the front matter, \
                            content replaces the whole file (front matter included), media sets a LinkedIn or X \
                            post's images or video. Published posts aren't edited here, except a posted LinkedIn \
                            or X post's media (where: published), so a post is recorded with what it showed.",
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
                            write them (generate: true), with the post's URL on the site. For a LinkedIn or X post \
                            of its own, on its network's list and calendar, use queue_linkedin or queue_x.",
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
            "description": "Publishes a draft: moves it into the blog (or LinkedIn's or X's posted folder), or clears \
                            its draft mark where drafts sit with published posts, dates it today (or published_at), \
                            and for a blog post runs the site's publish command, if it has one. A LinkedIn or X \
                            post is only recorded as posted: PublishKit doesn't post to them. A blog post whose checks \
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
                            `publishkit publish-due` runs. With kind linkedin or x, publish_at only plans the draft \
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
                            LinkedIn or X. For a post that went out at another time than PublishKit recorded. The \
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
    ])
}

/// `queue_linkedin` and `queue_x`.
fn social_tool(channel: Channel, project: &Value, lang: &Value, media: &Value) -> Value {
    let label = channel.label();
    let limit = match channel {
        Channel::X => " X takes 280 characters (longer with X Premium).",
        Channel::LinkedIn => " LinkedIn takes 3,000 characters.",
    };
    json!({
        "name": format!("queue_{}", channel.key()),
        "description": format!("Adds a {label} post to a language's queue, on its own, not tied to a blog post. \
                        Give body, the text as the user wrote it or as you wrote it with them; or generate: true \
                        with a topic, an angle and, if it links somewhere, a link to have PublishKit's model \
                        write it. A link goes in the body as the bare address, on a line of its own: {label} \
                        makes it clickable, and shows markdown links as they are typed. topic names it in the \
                        list; without one, its first line does. For a post already on {label}, give \
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
                "link": { "type": "string", "description": format!("For generate: an address the post \
                    links to. It is put in as it is, on a line of its own near the end: {label} makes it \
                    clickable, and shows markdown links as plain text.") },
                "generate": { "type": "boolean" },
                "published_at": { "type": "string", "description": format!("Already posted on {label}: \
                    when, 2026-10-16 or 2026-10-16T09:00. The post is saved straight among the posted ones.") },
                "media": media,
            },
            "required": ["lang"],
        },
    })
}

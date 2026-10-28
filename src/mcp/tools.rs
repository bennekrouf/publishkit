//! The tools PublishKit offers an MCP client, as the client sees them.

use super::*;

pub(super) fn tools() -> Value {
    let project = json!({ "type": "string", "description": "Absolute path of the site's folder. Defaults to the one open in PublishKit." });
    let lang = json!({ "type": "string", "description": "Language code, e.g. fr or en (project lists them)." });
    let filename = json!({ "type": "string", "description": "The post's file name as list_posts gives it: my-post.md, my-post.mdx, or my-post/index.md for a page bundle." });
    let kind =
        json!({ "type": "string", "enum": ["blog", "linkedin"], "description": "Default blog." });
    let place = json!({ "type": "string", "enum": ["queue", "published"], "description": "Default queue." });
    json!([
        {
            "name": "project",
            "description": "The site: its name, URL, author, default tags, its writing brief (the system prompt \
                            PublishKit writes with), the framework it is built with, its languages, and how many \
                            drafts and published posts each has, blog and LinkedIn. Also the model PublishKit \
                            generates with, and the other sites PublishKit knows (pass one's folder as project).",
            "inputSchema": { "type": "object", "properties": { "project": project } },
        },
        {
            "name": "list_posts",
            "description": "Posts with their title (or LinkedIn topic), description, date, status and length.",
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
            "description": "Changes a draft (blog or LinkedIn): body replaces what follows the front matter, \
                            content replaces the whole file (front matter included). Published posts aren't edited here.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind,
                    "body": { "type": "string" },
                    "content": { "type": "string" },
                },
                "required": ["lang", "filename"],
            },
        },
        {
            "name": "set_social",
            "description": "Sets a blog post's LinkedIn and X teasers, or has PublishKit's model write them \
                            (generate: true), with the post's URL on the site.",
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
            "description": "Publishes a draft: moves it into the blog (or LinkedIn's published folder), or clears its \
                            draft mark where drafts sit with published posts, dates it today, and for a blog post \
                            runs the site's publish command, if it has one. A blog post whose checks find problems \
                            (no title or text, TODO left in, a missing image) is refused with them; get_post lists \
                            its checks. Ask the user first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename, "kind": kind,
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
                            `publishkit publish-due` runs. Ask the user before scheduling.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project, "lang": lang, "filename": filename,
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
            "name": "queue_linkedin",
            "description": "Adds a standalone LinkedIn post to a language's queue. Give body, or generate: true with an \
                            angle to have PublishKit's model write it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "lang": lang,
                    "topic": { "type": "string" },
                    "body": { "type": "string" },
                    "angle": { "type": "string", "description": "Key points, for generate." },
                    "generate": { "type": "boolean" },
                },
                "required": ["lang", "topic"],
            },
        },
    ])
}

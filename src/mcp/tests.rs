use super::*;
use std::collections::HashSet;

#[test]
fn every_tool_has_a_unique_name_and_an_object_schema() {
    let tools = tools();
    let tools = tools.as_array().unwrap();
    let names: HashSet<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), tools.len());
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        for required in t["inputSchema"]["required"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let key = required.as_str().unwrap();
            assert!(
                t["inputSchema"]["properties"].get(key).is_some(),
                "{} requires {key} undescribed",
                t["name"]
            );
        }
    }
}

#[test]
fn a_file_name_never_leaves_its_folder() {
    assert_eq!(
        filename(&json!({ "filename": "post.md" })).unwrap(),
        "post.md"
    );
    assert_eq!(
        filename(&json!({ "filename": "post/index.mdx" })).unwrap(),
        "post/index.mdx"
    );
    for bad in [
        "../post.md",
        "a/b.md",
        ".hidden.md",
        "post.txt",
        "..\\x.md",
        "../index.md",
        "a/b/index.md",
        "/index.md",
    ] {
        assert!(filename(&json!({ "filename": bad })).is_err(), "{bad}");
    }
}

#[test]
fn a_body_keeps_the_front_matter() {
    let post = "---\ntitle: \"A\"\nstatus: \"draft\"\n---\n\n# A\n\nOld.";
    assert_eq!(
        split_front_matter(post),
        Some("---\ntitle: \"A\"\nstatus: \"draft\"\n---")
    );
    assert_eq!(split_front_matter("no front matter"), None);
    assert_eq!(
        split_front_matter("+++\ntitle = \"A\"\n+++\nBody"),
        Some("+++\ntitle = \"A\"\n+++")
    );
}

#[test]
fn the_desktop_entry_starts_this_app_in_mcp_mode() {
    let config: Value = serde_json::from_str(&desktop_config(
        "/Applications/PublishKit.app/Contents/MacOS/publishkit",
    ))
    .unwrap();
    assert_eq!(
        config["mcpServers"]["publishkit"]["command"],
        "/Applications/PublishKit.app/Contents/MacOS/publishkit"
    );
    assert_eq!(config["mcpServers"]["publishkit"]["args"], json!(["mcp"]));
    assert_eq!(
        code_command("/A B/publishkit"),
        "claude mcp add publishkit --scope user -- \"/A B/publishkit\" mcp"
    );
}

#[test]
fn the_codex_command_quotes_a_path_with_spaces() {
    assert_eq!(
        codex_command("/A B/publishkit"),
        "codex mcp add publishkit -- \"/A B/publishkit\" mcp"
    );
}

#[test]
fn every_network_has_a_queue_tool_and_a_kind() {
    let tools = tools();
    let tools = tools.as_array().unwrap();
    for channel in Channel::ALL {
        let name = format!("queue_{}", channel.key());
        assert!(tools.iter().any(|t| t["name"] == name.as_str()), "{name}");
        let get_post = tools.iter().find(|t| t["name"] == "get_post").unwrap();
        let kinds = get_post["inputSchema"]["properties"]["kind"]["enum"]
            .as_array()
            .unwrap();
        assert!(
            kinds.iter().any(|k| k == channel.key()),
            "{}",
            channel.key()
        );
    }
}

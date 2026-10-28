use super::*;

fn article() -> Article {
    let content = "---\ntitle: \"Hello, World\"\ndescription: \"Short.\"\ntags: [\"Rust\", \"web dev\", \"a\", \"b\", \"c\"]\n---\n\n\
                   # Hello, World\n\nSee ![x](/images/x.png) and [home](/en/blog/other).";
    Article::of(
        content,
        "https://site.org/",
        "https://site.org/en/blog/hello",
    )
}

#[test]
fn the_copy_points_at_the_site_and_leaves_the_title_to_the_platform() {
    let a = article();
    assert_eq!(a.title, "Hello, World");
    assert_eq!(
        a.markdown,
        "See ![x](https://site.org/images/x.png) and [home](https://site.org/en/blog/other)."
    );
    let bundle = Article::of(
        "---\ntitle: B\n---\n![c](cover.png) [x](#top)",
        "https://site.org",
        "https://site.org/blog/b/",
    );
    assert_eq!(
        bundle.markdown,
        "![c](https://site.org/blog/b/cover.png) [x](#top)"
    );
    assert!(a.html().contains("src=\"https://site.org/images/x.png\""));
}

#[test]
fn dev_to_gets_four_plain_tags_and_the_canonical_address() {
    let accounts = Accounts {
        devto_key: "k".into(),
        ..Default::default()
    };
    let r = devto_request(&accounts, &article(), None);
    assert_eq!(
        (r.method, r.url.as_str()),
        ("POST", "https://dev.to/api/articles")
    );
    assert_eq!(
        r.body["article"]["tags"],
        json!(["rust", "webdev", "a", "b"])
    );
    assert_eq!(
        r.body["article"]["canonical_url"],
        "https://site.org/en/blog/hello"
    );
    assert_eq!(r.body["article"]["published"], false);
    let again = devto_request(
        &accounts,
        &article(),
        Some(&Copy {
            id: "7".into(),
            url: String::new(),
        }),
    );
    assert_eq!(
        (again.method, again.url.as_str()),
        ("PUT", "https://dev.to/api/articles/7")
    );
}

#[test]
fn hashnode_drafts_then_updates_in_place() {
    let accounts = Accounts {
        hashnode_token: "t".into(),
        hashnode_publication: "p1".into(),
        ..Default::default()
    };
    let r = hashnode_request(&accounts, &article(), None);
    assert!(r.body["query"].as_str().unwrap().contains("createDraft"));
    assert_eq!(r.body["variables"]["input"]["publicationId"], "p1");
    assert_eq!(
        r.body["variables"]["input"]["tags"][1],
        json!({ "slug": "web-dev", "name": "web dev" })
    );
    let published = Copy {
        id: "abc".into(),
        url: "https://blog.x/hello".into(),
    };
    let r = hashnode_request(&accounts, &article(), Some(&published));
    assert!(r.body["query"].as_str().unwrap().contains("updatePost"));
    assert_eq!(r.body["variables"]["input"]["id"], "abc");
}

#[test]
fn wordpress_gets_html_with_a_link_to_the_original() {
    let accounts = Accounts {
        wordpress_url: "https://wp.org/".into(),
        wordpress_user: "me".into(),
        wordpress_password: "ab cd".into(),
        as_draft: false,
        ..Default::default()
    };
    let r = wordpress_request(&accounts, &article(), None);
    assert_eq!(r.url, "https://wp.org/wp-json/wp/v2/posts");
    assert_eq!(
        r.headers[0].1,
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode("me:abcd")
        )
    );
    assert_eq!(r.body["status"], "publish");
    assert!(r.body["content"].as_str().unwrap().ends_with(
        "<a href=\"https://site.org/en/blog/hello\">https://site.org/en/blog/hello</a>.</em></p>"
    ));
}

/// A WordPress that answers each request with the next id, and tells what it was asked.
async fn fake_wordpress() -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut id = 40;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut request = vec![0u8; 65536];
            let mut read = 0;
            // Headers, then as much body as Content-Length says.
            loop {
                let n = socket.read(&mut request[read..]).await.unwrap_or(0);
                read += n;
                let text = String::from_utf8_lossy(&request[..read]).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if read >= end + 4 + length || n == 0 {
                        break;
                    }
                } else if n == 0 {
                    break;
                }
            }
            id += 1;
            let first_line = String::from_utf8_lossy(&request[..read])
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            let _ = tx.send(first_line);
            let body = format!("{{\"id\": {id}, \"link\": \"https://wp.test/?p={id}\"}}");
            let answer = format!("HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = socket.write_all(answer.as_bytes()).await;
        }
    });
    (address, rx)
}

#[tokio::test]
async fn a_post_sent_twice_updates_its_copy() {
    let (address, mut asked) = fake_wordpress().await;
    let root = std::env::temp_dir().join(format!("publishkit-crosspost-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let project = root.to_string_lossy().into_owned();
    let accounts = Accounts {
        wordpress_url: address,
        wordpress_user: "me".into(),
        wordpress_password: "pw".into(),
        ..Default::default()
    };

    let first = crosspost(
        &project,
        &accounts,
        Target::WordPress,
        "en",
        "hello.md",
        &article(),
    )
    .await
    .unwrap();
    assert_eq!(
        first,
        Copy {
            id: "41".into(),
            url: "https://wp.test/?p=41".into()
        }
    );
    assert_eq!(
        asked.recv().await.unwrap(),
        "POST /wp-json/wp/v2/posts HTTP/1.1"
    );
    assert_eq!(
        Record::load(&project).copies("en", "hello.md")[&Target::WordPress],
        first
    );

    crosspost(
        &project,
        &accounts,
        Target::WordPress,
        "en",
        "hello.md",
        &article(),
    )
    .await
    .unwrap();
    assert_eq!(
        asked.recv().await.unwrap(),
        "POST /wp-json/wp/v2/posts/41 HTTP/1.1"
    );
    assert!(crosspost(
        &project,
        &Accounts::default(),
        Target::DevTo,
        "en",
        "hello.md",
        &article()
    )
    .await
    .is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn hmac_matches_rfc_4231() {
    let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(
        hex::encode(mac),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    let long_key = [0xaa_u8; 131];
    let mac = hmac_sha256(
        &long_key,
        b"Test Using Larger Than Block-Size Key - Hash Key First",
    );
    assert_eq!(
        hex::encode(mac),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn a_ghost_token_is_signed_with_the_secret() {
    let token = ghost_token("abc123:00ff", 1_700_000_000).unwrap();
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let header: Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(header["kid"], "abc123");
    let expected = hmac_sha256(
        &[0x00, 0xff],
        format!("{}.{}", parts[0], parts[1]).as_bytes(),
    );
    assert_eq!(
        parts[2],
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(expected)
    );
    assert!(ghost_token("no-colon", 0).is_err());
}

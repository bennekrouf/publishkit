use super::*;

#[test]
fn the_preview_finds_a_sites_images_and_leaves_others_alone() {
    let root = std::env::temp_dir().join(format!("publishkit-preview-{}", std::process::id()));
    std::fs::create_dir_all(root.join("public/images/blog")).unwrap();
    std::fs::create_dir_all(root.join("posts/one")).unwrap();
    std::fs::write(root.join("public/images/blog/a.png"), "png").unwrap();
    std::fs::write(root.join("posts/one/b.png"), "png").unwrap();
    let project = root.to_string_lossy().into_owned();
    let post = root.join("posts/one/index.md");
    let data = local_image(
        "/images/blog/a.png",
        &post,
        &project,
        "public/images/blog",
        "/images/blog",
    )
    .unwrap();
    assert_eq!(data, "data:image/png;base64,cG5n");
    assert!(local_image("b.png?v=2", &post, &project, "images", "/images").is_some());
    assert_eq!(
        local_image("https://x.org/a.png", &post, &project, "images", "/images"),
        None
    );
    assert_eq!(
        local_image("/missing.png", &post, &project, "images", "/images"),
        None
    );
    std::fs::remove_dir_all(root).unwrap();
}

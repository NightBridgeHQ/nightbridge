//! Embedded WebUI assets for the LocalSend Improved daemon.

use std::borrow::Cow;

use rust_embed::RustEmbed;

/// Embedded frontend asset.
#[derive(Clone, Debug)]
pub struct EmbeddedAsset {
    /// Asset bytes.
    pub data: Cow<'static, [u8]>,
    /// HTTP content type.
    pub content_type: &'static str,
}

#[derive(RustEmbed)]
#[folder = "dist/"]
struct Assets;

/// Return an embedded asset by path.
///
/// Empty paths and `/` resolve to `index.html`.
pub fn asset(path: &str) -> Option<EmbeddedAsset> {
    let path = normalize_path(path);
    Assets::get(path)
        .map(|file| EmbeddedAsset { data: file.data, content_type: content_type(path) })
}

fn normalize_path(path: &str) -> &str {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        "index.html"
    } else {
        path
    }
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_asset_is_embedded() {
        let asset = asset("index.html").expect("index.html should be embedded");

        assert_eq!(asset.content_type, "text/html; charset=utf-8");
        assert!(std::str::from_utf8(&asset.data).unwrap().contains("<div id=\"app\">"));
    }

    #[test]
    fn root_path_resolves_to_index() {
        assert!(asset("/").is_some());
    }
}

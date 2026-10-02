use std::path::{Component, Path};

pub fn extract_attr_value(content: &str, attr: &str) -> Vec<String> {
    let mut values = Vec::new();
    let needle = format!("{}=", attr);
    let mut rest = content;

    while let Some(idx) = rest.find(&needle) {
        rest = &rest[idx + needle.len()..];
        let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        rest = &rest[quote.len_utf8()..];
        let Some(end) = rest.find(quote) else {
            break;
        };
        values.push(rest[..end].trim().to_string());
        rest = &rest[end + quote.len_utf8()..];
    }

    values
}

pub fn resolve_html_asset_path(html_path: &str, asset_ref: &str) -> String {
    resolve_html_asset_path_or(html_path, asset_ref, "styles.css")
}

pub fn resolve_html_asset_path_or(html_path: &str, asset_ref: &str, fallback: &str) -> String {
    let stripped = asset_ref.trim_start_matches("./");
    let asset_path = Path::new(stripped);

    if asset_path.is_absolute() || stripped.contains("://") || !is_safe_relative_path(asset_path) {
        return fallback.to_string();
    }

    match html_path.rsplit_once('/') {
        Some((dir, _)) if !dir.is_empty() => format!("{}/{}", dir, stripped),
        _ => stripped.to_string(),
    }
}

pub fn is_safe_html_asset_ref(asset_ref: &str) -> bool {
    let stripped = asset_ref.trim_start_matches("./");
    let asset_path = Path::new(stripped);
    !stripped.contains("://") && is_safe_relative_path(asset_path)
}

pub fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && !path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

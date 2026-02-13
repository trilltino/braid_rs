use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use url::Url;

/// Convert a URL to a filesystem path within the given root directory.
pub fn url_to_path(root_dir: &Path, url_str: &str) -> Result<PathBuf> {
    let url = Url::parse(url_str)?;
    let host = url.host_str().ok_or_else(|| anyhow!("URL missing host"))?;
    let port = url.port();

    let mut domain_dir = host.to_string();
    if let Some(p) = port {
        // Use + instead of : for port separator on Windows (OS Error 123)
        domain_dir.push_str(&format!("+{}", p));
    }

    let mut path = root_dir.join(domain_dir);

    // Trim leading slash from path segments
    for segment in url.path_segments().unwrap_or_else(|| "".split('/')) {
        path.push(segment);
    }

    // If path ends in slash or is empty, it might be a directory in URL semantics
    if url.path().ends_with('/') {
        path.push("index");
    }

    Ok(path)
}

/// Convert a filesystem path to a URL, given the root directory.
pub fn path_to_url(root_dir: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root_dir)
        .map_err(|_| anyhow!("Path is not within root directory"))?;

    let mut components = relative.components();

    // First component is domain[:port]
    let domain_comp = components.next().ok_or_else(|| anyhow!("Path too short"))?;

    let domain_str = domain_comp.as_os_str().to_string_lossy();
    if domain_str.starts_with('.') {
        return Err(anyhow!("Ignoring dotfile/directory"));
    }

    let (host, port) = if let Some((h, p)) = domain_str.rsplit_once('+') {
        (h, Some(p.parse::<u16>()?))
    } else {
        (domain_str.as_ref(), None)
    };

    // Construct URL
    let scheme = if host == "localhost" || host == "127.0.0.1" {
        "http"
    } else {
        "https"
    };

    let mut url = Url::parse(&format!("{}://{}", scheme, host))?;
    if let Some(p) = port {
        url.set_port(Some(p)).map_err(|_| anyhow!("Invalid port"))?;
    }

    let mut path_segments = Vec::new();
    for comp in components {
        path_segments.push(comp.as_os_str().to_string_lossy().to_string());
    }

    if let Some(last) = path_segments.last() {
        if last == "index" {
            path_segments.pop();
        }
    }

    url.path_segments_mut()
        .map_err(|_| anyhow!("Cannot be base"))?
        .extend(path_segments);

    Ok(url.to_string())
}

pub fn path_join(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", parent, name)
    }
}

/// Extract markdown from HTML shell.
/// 
/// Looks for markdown content in a `<script type="text/markdown">` tag
/// or in an HTML comment `<!-- markdown: ... -->`.
/// If no markdown container is found, returns the content as-is.
pub fn extract_markdown(content: &str) -> String {
    // Try to find markdown in a script tag first
    if let Some(start) = content.find("<script type=\"text/markdown\">") {
        let start_idx = start + "<script type=\"text/markdown\">".len();
        if let Some(end) = content[start_idx..].find("</script>") {
            return content[start_idx..start_idx + end].trim().to_string();
        }
    }
    
    // Try HTML comment with markdown prefix
    if let Some(start) = content.find("<!-- markdown:") {
        let start_idx = start + "<!-- markdown:".len();
        if let Some(end) = content[start_idx..].find("-->") {
            return content[start_idx..start_idx + end].trim().to_string();
        }
    }
    
    // No markdown container found - check if it's already plain markdown
    // (heuristic: if it doesn't look like HTML, treat it as markdown)
    let trimmed = content.trim();
    if !trimmed.starts_with('<') || !trimmed.contains("</") {
        return content.to_string();
    }
    
    // Return content as-is for HTML files without markdown
    content.to_string()
}

/// Wrap markdown in HTML shell.
/// 
/// If old_content contains an HTML shell (has `<html>` or `<body>` tags),
/// replaces the markdown content within it. Otherwise, wraps the markdown
/// in a simple HTML shell.
pub fn wrap_markdown(old_content: &str, new_content: &str) -> String {
    // Check if old_content has an HTML shell
    let has_html_shell = old_content.trim().starts_with("<!DOCTYPE") 
        || old_content.trim().starts_with("<html") 
        || old_content.contains("<body");
    
    if !has_html_shell {
        // No shell - return markdown as-is
        return new_content.to_string();
    }
    
    // Try to replace content in script tag
    if let Some(start) = old_content.find("<script type=\"text/markdown\">") {
        let start_idx = start + "<script type=\"text/markdown\">".len();
        if let Some(end) = old_content[start_idx..].find("</script>") {
            let before = &old_content[..start_idx];
            let after = &old_content[start_idx + end..];
            return format!("{}{}{}", before, new_content, after);
        }
    }
    
    // Try to replace content in markdown comment
    if let Some(start) = old_content.find("<!-- markdown:") {
        let start_idx = start + "<!-- markdown:".len();
        if let Some(end) = old_content[start_idx..].find("-->") {
            let before = &old_content[..start_idx];
            let after = &old_content[start_idx + end..];
            return format!("{} {} {}{}", before, new_content.trim(), after, after);
        }
    }
    
    // No markdown container found - wrap in a simple HTML shell
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<title>Braid Document</title>
</head>
<body>
<script type="text/markdown">
{}
</script>
</body>
</html>"#,
        new_content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_to_path() {
        // Logic check
    }

    #[test]
    fn test_path_join() {
        assert_eq!(path_join("/", "foo"), "/foo");
        assert_eq!(path_join("/bar", "baz"), "/bar/baz");
    }
}

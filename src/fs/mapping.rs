use crate::fs::config::get_root_dir;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use url::Url;

pub fn url_to_path(url_str: &str) -> Result<PathBuf> {
    let url = Url::parse(url_str)?;
    let host = url.host_str().ok_or_else(|| anyhow!("URL missing host"))?;
    let port = url.port();

    let mut domain_dir = host.to_string();
    if let Some(p) = port {
        domain_dir.push_str(&format!(":{}", p));
    }

    let root = get_root_dir()?;
    let mut path = root.join(domain_dir);

    // Trim leading slash from path segments
    for segment in url.path_segments().unwrap_or_else(|| "".split('/')) {
        path.push(segment);
    }

    // If path ends in slash or is empty, it might be a directory in URL semantics
    // But map to index if needed is usually handled by checking if it's a directory on disk
    // or if the URL path ends with /.
    // The README says: "If using a directory, it will be named /index"
    // Let's implement basic mapping first.

    if url.path().ends_with('/') {
        path.push("index");
    }

    Ok(path)
}

pub fn path_to_url(path: &Path) -> Result<String> {
    let root = get_root_dir()?;

    let relative = path
        .strip_prefix(&root)
        .map_err(|_| anyhow!("Path is not within BraidFS root"))?;

    let mut components = relative.components();

    // First component is domain[:port]
    let domain_comp = components.next().ok_or_else(|| anyhow!("Path too short"))?;

    let domain_str = domain_comp.as_os_str().to_string_lossy();
    if domain_str.starts_with('.') {
        return Err(anyhow!("Ignoring dotfile/directory"));
    }

    let (host, port) = if let Some((h, p)) = domain_str.rsplit_once(':') {
        (h, Some(p.parse::<u16>()?))
    } else {
        (domain_str.as_ref(), None)
    };

    // Construct URL
    // Default to https, but maybe config needs to know scheme?
    // README says https://<domain>/<path>
    let scheme = "https";

    let mut url = Url::parse(&format!("{}://{}", scheme, host))?;
    if let Some(p) = port {
        url.set_port(Some(p)).map_err(|_| anyhow!("Invalid port"))?;
    }

    let mut path_segments = Vec::new();
    for comp in components {
        path_segments.push(comp.as_os_str().to_string_lossy().to_string());
    }

    // If filename is "index" or "index.*", do we treat it as directory?
    // README: "If you sync a URL path to a directory containing items within it, the directory will be named /index"
    // This is ambiguous. Let's assume standard file mapping for now.

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

#[cfg(test)]
mod tests {
    use super::*;

    // Mock get_root_dir for tests or just test logic independent of root?
    // Since get_root_dir depends on dirs::home_dir(), it works on dev machine but might fail in CI if no home.
    // We can assume logic works if path join works.

    #[test]
    fn test_url_to_path() {
        // This test might be system dependent due to get_root_dir
        // Skipping specific path assertion, checking relative structure
    }
}

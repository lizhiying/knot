use std::path::Path;

pub struct PathProcessor;

impl PathProcessor {
    /// Extracts the file name from a path string.
    pub fn extract_file_name(path_str: &str) -> String {
        Path::new(path_str)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    }

    /// Extracts tags from a path string by breaking it into components
    /// and filtering common system prefixes.
    pub fn extract_tags(path_str: &str) -> String {
        let path = Path::new(path_str);

        // Simple heuristic: Join all components with spaces to allow keyword match.
        // We filter out root slash, current dir dots, and common system roots to reduce noise.
        // Improve this later with Project Root detection if needed.
        path.components()
            .filter_map(|c| c.as_os_str().to_str())
            .filter(|s| {
                let s = *s;
                s != "/" && s != "\\" && s != "." && s != ".." && s != "Users" && s != "home"
            })
            // Filter out the username if possible? For now, just keep it simple.
            // Also filter out the file name itself, as that is covered by file_name field?
            // Yes, let's keep tags for *context* (folders).
            // But if we just join components, it includes filename.
            // Let's exclude the last component (filename) from tags?
            // "path_tags" usually implies the directory structure.
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Extract tags excluding the filename, just the directory structure.
    pub fn extract_directory_tags(path_str: &str) -> String {
        let path = Path::new(path_str);
        if let Some(parent) = path.parent() {
            Self::extract_tags(parent.to_str().unwrap_or(""))
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_name() {
        assert_eq!(PathProcessor::extract_file_name("/a/b/c.txt"), "c.txt");
        assert_eq!(PathProcessor::extract_file_name("c.txt"), "c.txt");
    }

    #[test]
    fn test_extract_tags() {
        let path = "/Users/lizhiying/Projects/knot/src/main.rs";
        let tags = PathProcessor::extract_tags(path);
        // "lizhiying Projects knot src main.rs"
        assert!(tags.contains("Projects"));
        assert!(tags.contains("knot"));
        assert!(tags.contains("src"));
    }

    #[test]
    fn test_extract_directory_tags() {
        let path = "/Users/lizhiying/Projects/knot/src/main.rs";
        let tags = PathProcessor::extract_directory_tags(path);
        assert!(tags.contains("Projects"));
        assert!(tags.contains("knot"));
        assert!(!tags.contains("main.rs"));
    }
}

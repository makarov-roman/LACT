use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn extract_css_classes(css: &str) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    let mut chars = css.chars().peekable();
    let mut in_comment = false;

    while let Some(c) = chars.next() {
        // Handle comment start /*
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_comment = true;
            continue;
        }
        // Handle comment end */
        if in_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_comment = false;
            }
            continue;
        }

        if c == '.' {
            let mut class_name = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_alphanumeric() || next == '-' || next == '_' {
                    class_name.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if !class_name.is_empty()
                && class_name
                .chars()
                .next()
                .map(|c| c.is_alphabetic() || c == '_')
                .unwrap_or(false)
            {
                classes.insert(class_name);
            }
        }
    }
    classes
}

pub fn collect_css_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_css_files(&path, files);
            } else if path.extension().and_then(|s| s.to_str()) == Some("css") {
                files.push(path);
            }
        }
    }
}

pub fn generate_css_classes_module(classes: &BTreeSet<String>) -> String {
    let mut code = String::from("// Auto-generated CSS class constants\n\n");

    for class in classes {
        code.push_str(&format!(
            "#[allow(non_upper_case_globals)]\npub const {}: &str = \"{}\";\n",
            class, class
        ));
    }

    code
}

pub fn combine_css_files(src_dir: &Path) -> (String, Vec<PathBuf>) {
    let mut css_files = Vec::new();
    collect_css_files(src_dir, &mut css_files);
    css_files.sort();

    let mut combined_css = String::new();

    for file in &css_files {
        let content = fs::read_to_string(file).expect("Could not read CSS file");
        combined_css.push_str(&format!("/* Source: {} */\n", file.display()));
        combined_css.push_str(&content);
        combined_css.push('\n');
    }

    (combined_css, css_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_classes_simple() {
        let css = ".foo { color: red; }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 1);
        assert!(classes.contains("foo"));
    }

    #[test]
    fn test_extract_css_classes_multiple() {
        let css = ".foo { } .bar { } .baz { }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 3);
        assert!(classes.contains("foo"));
        assert!(classes.contains("bar"));
        assert!(classes.contains("baz"));
    }

    #[test]
    fn test_extract_css_classes_with_pseudo() {
        let css = ".button:hover { } .link:active { }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 2);
        assert!(classes.contains("button"));
        assert!(classes.contains("link"));
    }

    #[test]
    fn test_extract_css_classes_ignores_comments() {
        let css = "/* Source: test.css */ .real_class { }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 1);
        assert!(classes.contains("real_class"));
        assert!(!classes.contains("css"));
    }

    #[test]
    fn test_extract_css_classes_ignores_numeric_start() {
        let css = ".valid { } .123invalid { } ._underscore { }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 2);
        assert!(classes.contains("valid"));
        assert!(classes.contains("_underscore"));
        assert!(!classes.contains("123invalid"));
    }

    #[test]
    fn test_extract_css_classes_with_dashes_and_underscores() {
        let css = ".my-class { } .my_class { } .my-mixed_class { }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 3);
        assert!(classes.contains("my-class"));
        assert!(classes.contains("my_class"));
        assert!(classes.contains("my-mixed_class"));
    }

    #[test]
    fn test_extract_css_classes_deduplicates() {
        let css = ".foo { } .foo:hover { } .foo { }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 1);
        assert!(classes.contains("foo"));
    }

    #[test]
    fn test_extract_css_classes_nested_selectors() {
        let css = ".parent .child { color: red; }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 2);
        assert!(classes.contains("parent"));
        assert!(classes.contains("child"));
    }

    #[test]
    fn test_extract_css_classes_combined_selectors() {
        let css = ".class_a.class_b { } .class_c, .class_d { }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 4);
        assert!(classes.contains("class_a"));
        assert!(classes.contains("class_b"));
        assert!(classes.contains("class_c"));
        assert!(classes.contains("class_d"));
    }

    #[test]
    fn test_extract_css_classes_child_combinator() {
        let css = ".wrapper > .inner { margin: 0; }";
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 2);
        assert!(classes.contains("wrapper"));
        assert!(classes.contains("inner"));
    }

    #[test]
    fn test_extract_css_classes_multiline() {
        let css = r#"
.container {
    padding: 10px;
    margin: 5px;
}

.header,
.footer {
    background: #fff;
}

/* Multi-line comment
   with .fake_class inside */
.real_class {
    color: red;
}
"#;
        let classes = extract_css_classes(css);
        assert_eq!(classes.len(), 4);
        assert!(classes.contains("container"));
        assert!(classes.contains("header"));
        assert!(classes.contains("footer"));
        assert!(classes.contains("real_class"));
        assert!(!classes.contains("fake_class"));
    }
}

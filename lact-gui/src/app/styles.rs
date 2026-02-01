pub const COMBINED_CSS: &str = include_str!(concat!(env!("OUT_DIR"), "/combined.css"));

pub mod classes {
    include!(concat!(env!("OUT_DIR"), "/css_classes.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_css_is_loaded() {
        assert!(!COMBINED_CSS.is_empty(), "Combined CSS should not be empty");
        assert!(
            COMBINED_CSS.contains(".app"),
            "Combined CSS should contain the .app class"
        );
    }

    #[test]
    fn test_css_classes_generated() {
        assert_eq!(classes::app, "app");
    }
}

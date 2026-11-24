//! In-page JavaScript sources, composed at compile time.
//!
//! The browser evaluates one arrow expression per `Runtime.evaluate`, so the
//! modules are only *lexically* split here: their function declarations share
//! one function scope when concatenated. Keep every module free of top-level
//! statements other than function declarations (the `extract-walk` module owns
//! the per-run `let` bindings and the final `return`).

/// One in-page pass: match, filter, extract computed styles and v2 facets.
pub const EXTRACT_JS: &str = concat!(
    include_str!("../js/extract.js"),
    include_str!("../js/extract-selectors.js"),
    include_str!("../js/extract-a11y.js"),
    include_str!("../js/extract-styles.js"),
    include_str!("../js/extract-facets.js"),
    include_str!("../js/extract-walk.js"),
);

/// Whole-page effect snapshot (before/after each interaction).
pub const CAPTURE_JS: &str = include_str!("../js/effects.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_js_has_one_arrow_and_balanced_braces() {
        assert!(EXTRACT_JS.starts_with("(args) => {"));
        // `atRuleText` splits on the literal string `'{'`, so the raw text
        // has one unmatched `{` that is not a JS block delimiter.
        assert_eq!(
            EXTRACT_JS.matches('{').count(),
            EXTRACT_JS.matches('}').count() + 1
        );
        // Functions called across module boundaries must be present exactly
        // once in the composed source (no duplicate top-level declarations).
        for name in [
            "function selectorOf(",
            "function ariaOf(",
            "function buildStyles(",
            "function collectDeclared(",
            "function buildNode(",
        ] {
            assert_eq!(
                EXTRACT_JS.matches(name).count(),
                1,
                "missing/duplicate {name}"
            );
        }
    }

    #[test]
    fn effects_js_is_self_contained() {
        assert!(CAPTURE_JS.starts_with("(args) => {"));
        assert_eq!(
            CAPTURE_JS.matches('{').count(),
            CAPTURE_JS.matches('}').count()
        );
    }
}

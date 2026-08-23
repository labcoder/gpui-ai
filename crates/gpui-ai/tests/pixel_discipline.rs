//! Raw-pixel discipline audit for library layout code.
//!
//! The upstream design guides (gpui-component, 2026-08) require application
//! layout to resolve through semantic spacing tokens or GPUI's rem scale
//! helpers instead of raw `px(...)` literals. Raw pixels are reserved for
//! physical/raster boundaries: measured geometry math, animation distance
//! computations, zero comparisons, theme token definitions, raster-frame
//! defaults, virtual-list overdraw, and test fixtures.
//!
//! This audit keeps the exception list explicit. If it fails, either the new
//! `px(` call belongs in the allowed list below (add a comment naming the
//! file and why), or the layout value should move onto
//! [`SemanticThemeTokens::spacing`](gpui_component::theme::SemanticThemeTokens).

use std::path::Path;

/// Files where raw `px(...)` is a deliberate, documented physical boundary.
const ALLOWED_FILES: &[&str] = &[
    // Scroll-distance arithmetic: pixels are the unit of scroll motion, not
    // layout spacing. Constants there are tuning parameters, not theme values.
    "crates/gpui-ai/src/scrolling.rs",
    // Semantic horizontal padding is applied with GPUI's `.px(token)` style
    // helper. The source contains `px(` text, but no raw pixel literal.
    "crates/gpui-ai/src/selection_actions.rs",
    // Raster-frame defaults: the generated-image frame mirrors an image's
    // intrinsic pixel dimensions, not UI spacing.
    "crates/gpui-ai/src/image_generation.rs",
    // Orb diameter is a canvas-like decorative radius tuned in pixels; the
    // default is exposed through the builder for callers who need other sizes.
    "crates/gpui-ai/src/orbs.rs",
    // Row-reorder motion arithmetic: offsets are animation distances from
    // rest (px(0.)), not layout spacing.
    "crates/gpui-ai/src/records_table.rs",
    // Virtual-list overdraw is a measured performance parameter (how far past
    // the viewport to keep rendered rows), independent of the type scale.
    // The same file also applies semantic padding with `.px(token)`.
    "crates/gpui-ai/src/chat.rs",
    // Theme definitions are where pixel values legitimately live; they feed
    // the token system rather than bypassing it.
    "crates/gpui-ai/src/theme.rs",
];

#[test]
fn library_layout_resolves_through_tokens_not_raw_pixels() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let src = Path::new(manifest).join("src");
    let mut violations = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(&src)
        .expect("library src directory should be readable")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    entries.sort();

    for path in entries {
        let name = path.to_string_lossy().replace('\\', "/");
        if ALLOWED_FILES.iter().any(|allowed| name.ends_with(allowed)) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("source should be readable");
        // Only the non-test portion of each module is audited; test fixtures
        // size windows and bounds in raw pixels by nature.
        let implementation = source
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields at least one part");
        for (index, line) in implementation.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            // `.px(` with a leading dot is the Styled padding helper taking
            // tokens; bare `px(` constructs a literal pixel value.
            if RegexLite::finds_literal_px(trimmed) {
                violations.push(format!(
                    "{}:{}: raw px() in layout code — use semantic spacing tokens \
                     or add the file to scrolling-style allowlist with justification\n  {}",
                    name,
                    index + 1,
                    trimmed
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "raw px() literals found outside documented physical boundaries:\n{}",
        violations.join("\n")
    );
}

/// Minimal matcher for a bare `px(` constructor call, ignoring `.px(`
/// builder methods and identifiers ending in `px` (e.g. `rem_px(`).
struct RegexLite;

impl RegexLite {
    fn finds_literal_px(line: &str) -> bool {
        let bytes = line.as_bytes();
        let mut position = 0;
        while let Some(offset) = line[position..].find("px(") {
            let start = position + offset;
            let previous_ok = start == 0 || {
                let previous = bytes[start - 1];
                !(previous == b'.' || previous.is_ascii_alphanumeric() || previous == b'_')
            };
            if previous_ok {
                return true;
            }
            position = start + 3;
        }
        false
    }
}

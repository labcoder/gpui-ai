//! Exports the component catalog the website is generated from.
//!
//! ```sh
//! npm run generate:catalog
//! ```
//!
//! Writes `site/generated/catalog.json` from [`StoryId`] and the library
//! sources, replacing a hand-maintained list that could drift from the stories
//! it described. Event names are read out of `crates/gpui-ai/src` rather than
//! restated here, so a renamed event enum shows up as a catalog diff.

use gallery::{StoryId, Viewport};
use serde_json::{Map, Value, json};
use std::{fs, path::PathBuf};

/// Every browser demo carries the same caveat; the native runtime is the one
/// that has been verified.
const LIMITATION: &str = "The live browser specimen requires WebGPU; the native component remains the authoritative runtime.";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the gallery crate lives two directories below the workspace root")
        .to_path_buf()
}

/// Reads the public event enums a component declares, in declaration order.
fn events(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("pub enum ") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if name.ends_with("Event") {
            found.push(name);
        }
    }
    found
}

fn join_events(events: &[String]) -> String {
    match events {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Collects the code between `// snippet:start(<slug>[/<variant>])` and
/// `// snippet:end` markers, keyed by story slug then variant.
///
/// Snippets are cut from the gallery's own compiled source rather than written
/// beside it, so a story that changes shape cannot leave the website showing
/// code that no longer builds. A story may mark several regions for one
/// variant — an entity's construction and the place it is mounted, say — and
/// they are joined in source order.
fn snippets(source: &str) -> Vec<(String, String, String)> {
    let mut collected: Vec<(String, String, String)> = Vec::new();
    let mut open: Option<(String, String, Vec<&str>)> = None;

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("// snippet:start(") {
            let key = rest.strip_suffix(')').unwrap_or(rest);
            let (slug, variant) = key.split_once('/').unwrap_or((key, "default"));
            assert!(
                open.is_none(),
                "snippet:start inside an open snippet ({slug})"
            );
            open = Some((slug.to_owned(), variant.to_owned(), Vec::new()));
            continue;
        }
        if trimmed == "// snippet:end" {
            let (slug, variant, body) = open.take().expect("snippet:end without snippet:start");
            let indent = body
                .iter()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.len() - line.trim_start().len())
                .min()
                .unwrap_or(0);
            let code = body
                .iter()
                .map(|line| {
                    if line.len() > indent {
                        &line[indent..]
                    } else {
                        line.trim_start()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                .trim_end()
                .to_owned();
            assert!(!code.is_empty(), "{slug} marks an empty snippet");

            match collected
                .iter_mut()
                .find(|(existing, existing_variant, _)| {
                    *existing == slug && *existing_variant == variant
                }) {
                Some((_, _, existing)) => {
                    existing.push_str("\n\n");
                    existing.push_str(&code);
                }
                None => collected.push((slug, variant, code)),
            }
            continue;
        }
        if let Some((_, _, body)) = open.as_mut() {
            body.push(line);
        }
    }

    assert!(open.is_none(), "a snippet was opened and never closed");
    collected
}

fn main() {
    let root = workspace_root();
    let mut components = Vec::new();

    for (index, story) in StoryId::ALL.iter().enumerate() {
        let meta = story
            .meta()
            .unwrap_or_else(|| panic!("{} has no catalog metadata", story.slug()));

        let source = format!("crates/gpui-ai/src/{}.rs", meta.module);
        let text = fs::read_to_string(root.join(&source))
            .unwrap_or_else(|error| panic!("cannot read {source}: {error}"));
        assert!(
            text.contains(&format!("pub struct {}", meta.api)),
            "{source} does not declare pub struct {}",
            meta.api
        );

        let events = events(&text);
        let interaction = if events.is_empty() {
            "This presentation surface adds no component-specific interaction event.".to_owned()
        } else {
            format!(
                "Interactive intent is reported through the typed {} contract{} and stable application IDs.",
                join_events(&events),
                if events.len() > 1 { "s" } else { "" }
            )
        };
        let overflow = match meta.viewport {
            Viewport::Tall => {
                "Growing content remains reachable in a bounded vertical surface; reduced motion preserves a useful state."
            }
            Viewport::Wide => {
                "Wide content retains context in a bounded surface; reduced motion preserves a useful state."
            }
        };

        let variants: Vec<Value> = story
            .variants()
            .iter()
            .map(|(id, label)| json!({ "id": id, "label": label }))
            .collect();

        components.push(json!({
            "sequence": index + 1,
            "slug": story.slug(),
            "title": story.title(),
            "compactLabel": story.title(),
            "category": meta.category,
            "summary": meta.summary,
            "source": source,
            "api": meta.api,
            "usage": meta.usage,
            "viewport": meta.viewport.as_str(),
            // The window frame the site draws around each demo.
            "windowTitle": format!("{} — gpui-ai", story.title()),
            "variants": variants,
            "events": events,
            "event": events.first().cloned(),
            "limitation": LIMITATION,
            "behavior": {
                "ownership": format!(
                    "{} renders state supplied by the application; it does not own durable work.",
                    meta.api
                ),
                "interaction": interaction,
                "semantics": meta.summary,
                "overflow": overflow,
            },
        }));
    }

    let mut categories = Vec::new();
    for component in &components {
        let category = component["category"].clone();
        if !categories.contains(&category) {
            categories.push(category);
        }
    }

    let mut document = Map::new();
    document.insert(
        "$comment".to_owned(),
        Value::String(
            "Generated by gallery-catalog-export from crates/gallery/src/story.rs. Do not edit."
                .to_owned(),
        ),
    );
    document.insert("categories".to_owned(), Value::Array(categories));
    document.insert("components".to_owned(), Value::Array(components));

    let output = root.join("site").join("generated");
    fs::create_dir_all(&output).expect("site/generated must be writable");
    let path = output.join("catalog.json");
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(&Value::Object(document)).expect("the catalog must serialize")
    );
    fs::write(&path, rendered).expect("the catalog must be writable");

    let gallery_source = root
        .join("crates")
        .join("gallery")
        .join("src")
        .join("gallery.rs");
    let gallery = fs::read_to_string(&gallery_source).expect("the gallery source must be readable");
    let mut by_story: Map<String, Value> = Map::new();
    for (slug, variant, code) in snippets(&gallery) {
        assert!(
            StoryId::ALL.iter().any(|story| story.slug() == slug),
            "snippet marker names {slug}, which is not a story"
        );
        by_story
            .entry(slug)
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("snippet entries are objects")
            .insert(variant, Value::String(code));
    }

    for story in StoryId::ALL {
        assert!(
            by_story.contains_key(story.slug()),
            "{} has no // snippet:start marker; every component page needs one",
            story.slug()
        );
    }

    let mut snippet_document = Map::new();
    snippet_document.insert(
        "$comment".to_owned(),
        Value::String(
            "Generated by gallery-catalog-export from the gallery's own source. Do not edit."
                .to_owned(),
        ),
    );
    snippet_document.insert("snippets".to_owned(), Value::Object(by_story));
    fs::write(
        output.join("snippets.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&Value::Object(snippet_document))
                .expect("the snippets must serialize")
        ),
    )
    .expect("the snippets must be writable");

    println!(
        "generated site/generated/catalog.json and snippets.json: {} components",
        StoryId::ALL.len()
    );
}

//! Living Specification generator — renders the site's `spec.html` from the
//! shared `spec/anzan/*.feature` files, **engine-neutral** (Rust-rendered),
//! matching the Swift/PickleKit page structure and the shared design tokens so
//! the two engines' verification pages read as one family.
//!
//! It parses the Gherkin directly (no test run) via the `gherkin` parser that
//! `cucumber` re-exports, so it is a `harness = false` test (examples/bins can't
//! see the `cucumber` dev-dependency). It is **env-gated**: a no-op unless
//! `SOROBAN_SPEC=<out.html>` is set, so a plain `cargo test` is unchanged.
//!
//!   SOROBAN_SPEC=site/public/spec.html \
//!     cargo test -p soroban-engine --test living_spec
//!
//! The living spec is the neutral front door; it cross-links BOTH engine
//! reports — `report.html` (native Swift) and `rust-report.html` (Rust).

use cucumber::gherkin::{Feature, GherkinEnv, Step};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

const CSS: &str = include_str!("living_spec.css");
const HEAD_JS: &str = include_str!("living_spec.head.js");
const FOOT_JS: &str = include_str!("living_spec.foot.js");

/// The shared feature files, relative to this crate (mirrors the gherkin test's
/// `../../spec/anzan` and the `author_interchange` example's path idiom).
const SPEC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec/anzan");

fn main() {
    let Ok(out) = std::env::var("SOROBAN_SPEC") else {
        return; // inert unless asked to generate
    };
    let features = load_features();
    let (html, f, b, s) = render(&features);
    fs::write(&out, html).unwrap_or_else(|e| panic!("cannot write SOROBAN_SPEC '{out}': {e}"));
    println!("Wrote {out} — {f} features · {b} behaviors · {s} steps.");
}

/// Parse every `*.feature` under `spec/anzan`, sorted by filename (which is the
/// order the page's rail presents — anzan, calculation, … structures).
fn load_features() -> Vec<Feature> {
    let dir = PathBuf::from(SPEC_DIR);
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "feature"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            Feature::parse_path(p, GherkinEnv::default())
                .unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
        })
        .collect()
}

/// One rendered behavior: a display name, its (background-folded, substituted)
/// steps, and whether it is verified or a `@rust-pending` skip.
struct Behavior {
    name: String,
    steps: Vec<(String, String)>, // (keyword, text)
    skipped: bool,
}

/// A feature's scenarios, grouped: a standalone scenario is a group of one; a
/// scenario outline is a group of its expanded example cases.
struct Group {
    outline: Option<(String, usize)>, // (outline name, case count) — None for standalone
    behaviors: Vec<Behavior>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Fold the feature background (if any) ahead of a scenario's own steps.
fn fold(background: &[Step], steps: &[Step]) -> Vec<Step> {
    background.iter().chain(steps).cloned().collect()
}

fn as_pair(step: &Step) -> (String, String) {
    (step.keyword.trim().to_string(), step.value.clone())
}

/// Substitute `<col>` placeholders in a step's text with an example row's cells.
fn substitute(text: &str, headers: &[String], row: &[String]) -> String {
    let mut out = text.to_string();
    for (h, cell) in headers.iter().zip(row) {
        out = out.replace(&format!("<{h}>"), cell);
    }
    out
}

/// Build the groups for one feature (folding background, expanding outlines).
fn groups(feature: &Feature) -> Vec<Group> {
    let background: Vec<Step> = feature
        .background
        .as_ref()
        .map(|b| b.steps.clone())
        .unwrap_or_default();
    let feature_pending = feature.tags.iter().any(|t| t == "rust-pending");

    feature
        .scenarios
        .iter()
        .map(|scenario| {
            let skipped = feature_pending || scenario.tags.iter().any(|t| t == "rust-pending");
            let folded = fold(&background, &scenario.steps);

            // A scenario outline carries one or more Examples tables; expand
            // each data row into its own behavior (named by joining the row's
            // cells, matching PickleKit).
            if scenario.examples.iter().any(|ex| ex.table.is_some()) {
                let mut behaviors: Vec<Behavior> = Vec::new();
                for table in scenario.examples.iter().filter_map(|ex| ex.table.as_ref()) {
                    let headers = table.rows.first().cloned().unwrap_or_default();
                    for row in table.rows.iter().skip(1) {
                        let steps = folded
                            .iter()
                            .map(|s| {
                                (
                                    s.keyword.trim().to_string(),
                                    substitute(&s.value, &headers, row),
                                )
                            })
                            .collect();
                        behaviors.push(Behavior {
                            name: row.join(", "),
                            steps,
                            skipped,
                        });
                    }
                }
                Group {
                    outline: Some((scenario.name.clone(), behaviors.len())),
                    behaviors,
                }
            } else {
                Group {
                    outline: None,
                    behaviors: vec![Behavior {
                        name: scenario.name.clone(),
                        steps: folded.iter().map(as_pair).collect(),
                        skipped,
                    }],
                }
            }
        })
        .collect()
}

/// Render the full page; returns (html, feature_count, behavior_count, step_count).
fn render(features: &[Feature]) -> (String, usize, usize, usize) {
    let per_feature: Vec<(&Feature, Vec<Group>)> =
        features.iter().map(|f| (f, groups(f))).collect();

    let feature_count = per_feature.len();
    let behavior_count: usize = per_feature
        .iter()
        .flat_map(|(_, gs)| gs.iter())
        .map(|g| g.behaviors.len())
        .sum();
    let step_count: usize = per_feature
        .iter()
        .flat_map(|(_, gs)| gs.iter())
        .flat_map(|g| g.behaviors.iter())
        .map(|b| b.steps.len())
        .sum();
    let skipped_count: usize = per_feature
        .iter()
        .flat_map(|(_, gs)| gs.iter())
        .flat_map(|g| g.behaviors.iter())
        .filter(|b| b.skipped)
        .count();
    let verified = behavior_count - skipped_count;

    // ---- header ----
    let mut body = String::new();
    let scale =
        format!("{feature_count} features · {behavior_count} behaviors · {step_count} steps");
    let verified_line = if skipped_count == 0 {
        format!("<span class=\"check\">✓</span> {scale} — <strong>every one verified</strong>")
    } else {
        format!("{scale} — <strong>{verified} of {behavior_count} verified</strong>, {skipped_count} pending")
    };
    write!(
        body,
        "<header class=\"page-header\"><div class=\"page-header-row\">\
<div class=\"page-header-left\">\
<button class=\"icon-btn\" onclick=\"toggleRail()\" aria-label=\"Toggle sidebar\" title=\"Toggle sidebar\">☰</button>\
<h1>Anzan — Living Specification</h1></div>\
<button class=\"icon-btn\" onclick=\"cycleTheme()\" aria-label=\"Toggle theme\" title=\"Toggle light/dark\">◐</button>\
</div><p class=\"page-sub\">{verified_line}</p>\
<p class=\"cross-link\">Verified by two engines against one shared spec: \
<a href=\"report.html\">native (Swift) ↗</a> · \
<a href=\"rust-report.html\">cross-platform (Rust) ↗</a></p></header>"
    )
    .unwrap();

    // ---- layout: rail + body ----
    body.push_str("<div class=\"page-layout\"><nav class=\"rail\" aria-label=\"Features\">\n  <h2>Features</h2>\n  <ul>\n    ");
    for (i, (feature, gs)) in per_feature.iter().enumerate() {
        let all_skipped = gs
            .iter()
            .flat_map(|g| g.behaviors.iter())
            .all(|b| b.skipped);
        let dot = if all_skipped { "skipped" } else { "passed" };
        writeln!(
            body,
            "<li><a href=\"#feature-{i}\" data-target=\"feature-{i}\" onclick=\"jumpTo('feature-{i}'); return false;\"><span class=\"dot {dot}\"></span>{}</a></li>",
            esc(&feature.name)
        )
        .unwrap();
    }
    body.push_str("  </ul>\n</nav><div class=\"page-body\">");

    // ---- feature sections ----
    for (i, (feature, gs)) in per_feature.iter().enumerate() {
        let count: usize = gs.iter().map(|g| g.behaviors.len()).sum();
        let pending: usize = gs
            .iter()
            .flat_map(|g| g.behaviors.iter())
            .filter(|b| b.skipped)
            .count();
        let badge = if pending == 0 {
            format!("<span class=\"verified ok\">✓ {count} examples</span>")
        } else {
            format!(
                "<span class=\"verified ok\">✓ {} of {count} verified</span>",
                count - pending
            )
        };
        write!(
            body,
            "<section class=\"spec-feature\" id=\"feature-{i}\"><div class=\"feature-head\"><h2>{}</h2>{badge}</div>",
            esc(&feature.name)
        )
        .unwrap();
        if let Some(desc) = &feature.description {
            let desc = desc.trim();
            if !desc.is_empty() {
                write!(body, "<p class=\"narrative\">{}</p>", esc(desc)).unwrap();
            }
        }
        if !feature.tags.is_empty() {
            body.push_str("<div class=\"tags\">");
            for t in &feature.tags {
                write!(body, "<span class=\"tag\">@{}</span>", esc(t)).unwrap();
            }
            body.push_str("</div>");
        }
        for group in gs {
            render_group(&mut body, group);
        }
        body.push_str("</section>");
    }
    body.push_str("</div></div>"); // page-body, page-layout

    // ---- assemble document ----
    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\" />\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    html.push_str("<title>Anzan — Living Specification</title>\n");
    writeln!(html, "<script>{HEAD_JS}</script>").unwrap();
    writeln!(html, "<style>{CSS}</style>").unwrap();
    html.push_str("</head>\n<body>");
    html.push_str(&body);
    write!(html, "<script>{FOOT_JS}</script>").unwrap();
    html.push_str("</body>\n</html>\n");

    (html, feature_count, behavior_count, step_count)
}

fn render_group(body: &mut String, group: &Group) {
    match &group.outline {
        Some((name, cases)) => {
            let status = if group.behaviors.iter().all(|b| b.skipped) {
                "skipped"
            } else {
                "passed"
            };
            write!(
                body,
                "<details class=\"outline-group\" data-status=\"{status}\"><summary><span class=\"outline-name\">{}</span><span class=\"outline-badge\">outline · {cases}</span></summary><div class=\"outline-cases\">",
                esc(name)
            )
            .unwrap();
            for b in &group.behaviors {
                render_scenario(body, b);
            }
            body.push_str("</div></details>");
        }
        None => {
            for b in &group.behaviors {
                render_scenario(body, b);
            }
        }
    }
}

fn render_scenario(body: &mut String, b: &Behavior) {
    let (status, mark) = if b.skipped {
        ("skipped", "○")
    } else {
        ("passed", "✓")
    };
    write!(
        body,
        "<details class=\"scenario {status}\"><summary><span class=\"mark {status}\">{mark}</span><span class=\"scenario-name\">{}</span></summary><div class=\"steps\">",
        esc(&b.name)
    )
    .unwrap();
    if b.steps.is_empty() {
        body.push_str("<div class=\"step muted\">(no steps recorded)</div>");
    } else {
        for (kw, text) in &b.steps {
            write!(
                body,
                "<div class=\"step\"><span class=\"kw\">{}</span> <span class=\"txt\">{}</span></div>",
                esc(kw),
                esc(text)
            )
            .unwrap();
        }
    }
    body.push_str("</div></details>");
}

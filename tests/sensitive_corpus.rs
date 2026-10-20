// SPDX-License-Identifier: Apache-2.0

//! The original's sensitive-clip corpus, generated at test time.
//!
//! `tests/support/sensitive_corpus.py` is Clipman's seeded generator (commit
//! 36784cc, Apache-2.0). Running it produces 568 benign clips and 167
//! synthetic secrets without a single secret literal living in this
//! repository. The same assertions as the original's `test_sensitive.py`
//! apply: no benign clip may be flagged, the only missed categories are the
//! four the detector skips on purpose, and recall stays at or above the
//! original's floor.

use std::collections::BTreeSet;
use std::process::Command;

use cosmic_clipman::sensitive::is_sensitive;

fn corpus() -> serde_json::Value {
    let dir = tempfile::tempdir().expect("temp dir");
    let script = dir.path().join("sensitive_corpus.py");
    std::fs::copy("tests/support/sensitive_corpus.py", &script)
        .expect("the vendored corpus generator exists");
    let status = Command::new("python3")
        .arg(&script)
        .current_dir(dir.path())
        .status()
        .expect("python3 is needed to generate the sensitive corpus");
    assert!(status.success(), "the corpus generator failed");
    let raw = std::fs::read_to_string(dir.path().join("corpus.json"))
        .expect("the generator wrote corpus.json");
    serde_json::from_str(&raw).expect("the corpus is valid JSON")
}

fn texts(section: &serde_json::Value) -> Vec<(String, String)> {
    section
        .as_array()
        .expect("the section is an array")
        .iter()
        .map(|entry| {
            (
                entry["category"].as_str().unwrap_or("").to_owned(),
                entry["text"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect()
}

#[test]
fn no_benign_clip_is_flagged() {
    let corpus = corpus();
    let flagged: Vec<String> = texts(&corpus["benign"])
        .into_iter()
        .filter(|(_, text)| is_sensitive(text))
        .map(|(category, text)| format!("{category}: {text}"))
        .collect();
    assert!(
        flagged.is_empty(),
        "{} benign clips flagged: {flagged:?}",
        flagged.len()
    );
}

#[test]
fn every_detected_category_has_a_hit() {
    let corpus = corpus();
    let missed: BTreeSet<String> = texts(&corpus["secret"])
        .into_iter()
        .filter(|(_, text)| !is_sensitive(text))
        .map(|(category, _)| category)
        .collect();
    let expected: BTreeSet<String> = [
        "password",
        "passphrase",
        "password_generated",
        "aws_secret_access_key",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(missed, expected);
}

#[test]
fn recall_floor() {
    let corpus = corpus();
    let secrets = texts(&corpus["secret"]);
    let caught = secrets
        .iter()
        .filter(|(_, text)| is_sensitive(text))
        .count();
    assert!(
        caught >= 131,
        "{caught}/{} caught, below the original's floor",
        secrets.len()
    );
}

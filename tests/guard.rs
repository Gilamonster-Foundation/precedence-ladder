//! **The dependency direction is armed, not asserted in a comment.**
//!
//! Two claims this crate makes about itself are load-bearing for every
//! consumer, and prose has never once enforced one:
//!
//! 1. **The core predicate depends on nothing.** With `--no-default-features`
//!    the resolved dependency closure is *empty*, so a wasm or embedded
//!    consumer pays for `resolve` and nothing else.
//! 2. **It takes no ambient authority.** No `std::fs`, `std::net`,
//!    `std::process`, or `std::env` anywhere in `src/` — "filesystem" is not a
//!    crate name, so no closure walk can ever see it, which is why this half
//!    scans source.
//!
//! Each half carries an **ANTI-VACUOUS TWIN** that points the same machinery
//! at a target known to violate it. A guard that cannot fail is decoration.
//!
//! These are ordinary `cargo test`s, so they run on every PR — not only at
//! release time.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Crates this predicate must never reach. Terminal, async, and application
/// libraries: a `KeyEvent` in a signature forks the crate on day one, and a
/// runtime in the closure ends the "pure predicate" claim.
const FORBIDDEN: &[&str] = &[
    "crossterm",
    "ratatui",
    "termion",
    "tokio",
    "async-std",
    "reqwest",
    "hyper",
    "axum",
    "rusqlite",
    "clap",
];

/// Ambient authority this crate must never take. These are capabilities, not
/// crate names.
const FORBIDDEN_STD: &[&str] = &["fs", "net", "process", "env"];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The transitive SHIPPED-dependency closure of this crate under `features`,
/// by name, excluding the crate itself.
///
/// Normal **and** build dependencies count. A build script runs with the full
/// authority of the building user — reading the filesystem, spawning
/// processes, reaching the network — before a line of the crate compiles, so a
/// forbidden crate arriving through `[build-dependencies]` is not a
/// technicality; it is the guard's whole subject coming in another door. Only
/// `dev` is excluded, because dev-dependencies impose nothing on a consumer.
///
/// Uses `cargo tree` rather than walking `cargo metadata`'s resolve graph by
/// hand: cargo already computes feature-resolved closures correctly, and a
/// hand-rolled walk here would be a second implementation of it — plus a
/// dev-dependency on a JSON parser this crate is otherwise proud not to need.
fn closure(features: &[&str]) -> BTreeSet<String> {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["tree", "--edges", "normal,build", "--prefix", "none"])
        .args(["--format", "{p}", "--manifest-path"])
        .arg(manifest_dir().join("Cargo.toml"))
        .arg("--no-default-features");
    if !features.is_empty() {
        cmd.args(["--features", &features.join(",")]);
    }
    let out = cmd.output().expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        // `[build-dependencies]` section headers, and the crate itself.
        .filter(|name| !name.starts_with('[') && *name != "precedence-ladder")
        .map(str::to_string)
        .collect()
}

/// The forbidden names present in `closure`.
fn violations(closure: &BTreeSet<String>, forbidden: &[&str]) -> Vec<String> {
    closure
        .iter()
        .filter(|name| forbidden.contains(&name.as_str()))
        .cloned()
        .collect()
}

/// **The core predicate is a true leaf: its closure is EMPTY.**
///
/// Not "small", not "registry-only" — empty. `resolve`, `describe`,
/// `collisions` and `claimants` are pure functions over `String` and
/// `BTreeSet`, and this is the assertion that keeps them that way. The moment
/// a non-optional dependency appears in `Cargo.toml`, or a feature stops
/// gating one, this goes red.
#[test]
fn the_core_predicate_depends_on_nothing() {
    let bare = closure(&[]);
    assert!(
        bare.is_empty(),
        "`--no-default-features` pulls in {bare:?}. The core predicate must \
         depend on NOTHING: that is what lets a wasm or embedded consumer pay \
         for `resolve` and nothing else. If the new dependency is real, put it \
         behind a feature."
    );
}

/// **ANTI-VACUOUS TWIN for the emptiness claim.** The same walker, pointed at
/// the default feature set, must come back non-empty and must name `serde` and
/// `toml`. An empty result from a walker that always returns empty proves
/// nothing at all — and "the closure is empty" is exactly the shape of claim
/// that a broken reader satisfies for free.
#[test]
fn the_walker_sees_the_dependencies_that_do_exist() {
    let with_table = closure(&["table"]);
    assert!(
        !with_table.is_empty(),
        "the walker returned nothing for the `table` feature — it is not \
         reading the dependency graph, so `the_core_predicate_depends_on_nothing` \
         is vacuous"
    );
    for expected in ["serde", "toml"] {
        assert!(
            with_table.contains(expected),
            "the `table` closure is missing {expected:?}: {with_table:?}"
        );
    }
    let with_cid = closure(&["cid"]);
    assert!(
        with_cid.contains("content-addressable"),
        "the `cid` closure is missing content-addressable: {with_cid:?}. \
         Identity is minted by that crate, never by a hand-rolled digest."
    );
}

/// **No terminal, runtime, or application crate, at any feature setting.**
#[test]
fn no_feature_reaches_a_forbidden_crate() {
    let all = closure(&["table", "cid"]);
    let found = violations(&all, FORBIDDEN);
    assert!(
        found.is_empty(),
        "precedence-ladder reaches {found:?}. A trigger is an opaque string \
         and the crate owns no terminal, no runtime, and no CLI; any of these \
         in the closure means one of those boundaries moved. Closure: {all:?}"
    );

    // ANTI-VACUOUS TWIN: the same predicate over the same REAL closure, with a
    // name that is genuinely in it. If this reports nothing, the check above
    // would report nothing whatever the closure contained.
    assert!(
        !violations(&all, &["serde"]).is_empty(),
        "the forbidden-name check cannot see a crate that is demonstrably in \
         the closure: {all:?}"
    );
}

/// **Every runtime dependency is optional.**
///
/// The closure check forbids specific crates; this forbids ADDITION. A
/// non-optional entry would ship to every consumer including the
/// `--no-default-features` one, and would end the empty-closure claim above —
/// so the DEFAULT manifest is pinned, not merely the absence of a bad name.
#[test]
fn the_declared_runtime_dependencies_are_exactly_three_and_all_optional() {
    let manifest = read_manifest();
    let (declared, optional) = dependency_table(&manifest);
    assert_eq!(
        declared,
        Vec::<String>::new(),
        "a NON-optional runtime dependency appeared. Everything this crate \
         depends on must be behind a feature, or `--no-default-features` stops \
         being empty."
    );
    assert_eq!(
        optional,
        vec![
            "content-addressable".to_string(),
            "serde".to_string(),
            "toml".to_string()
        ],
        "the runtime dependency set changed. Anything new here ships to every \
         consumer that enables its feature; justify it in the PR."
    );
}

/// **ANTI-VACUOUS TWIN for the manifest parser.** Point it at a table that
/// really does declare a non-optional dependency and it must say so. A parser
/// that returns two empty lists satisfies half the assertions above for free.
#[test]
fn the_manifest_parser_would_notice_a_non_optional_dependency() {
    let (declared, optional) = dependency_table(
        "[package]\nname = \"x\"\n\n[dependencies]\n\
         # a comment\n\
         tokio = \"1\"\n\
         serde = { version = \"1\", optional = true }\n\
         toml.workspace = true\n\
         \n[dev-dependencies]\nnever_seen = \"1\"\n",
    );
    assert_eq!(declared, vec!["tokio".to_string(), "toml".to_string()]);
    assert_eq!(optional, vec!["serde".to_string()]);
}

fn read_manifest() -> String {
    std::fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("the manifest is readable")
}

/// `(non-optional, optional)` dependency names from a manifest's
/// `[dependencies]` table, both sorted.
fn dependency_table(manifest: &str) -> (Vec<String>, Vec<String>) {
    let table = manifest
        .split("\n[dependencies]")
        .nth(1)
        .expect("a [dependencies] table")
        .split("\n[")
        .next()
        .expect("the table ends");

    let mut declared = Vec::new();
    let mut optional = Vec::new();
    // A `key = { … }` entry may wrap across lines; join continuation lines
    // onto the entry that opened the brace.
    let mut entry = String::new();
    for line in table.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        entry.push_str(line);
        if entry.matches('{').count() > entry.matches('}').count() {
            continue; // still inside a wrapped inline table
        }
        if let Some((name, rest)) = entry.split_once(['=', '.']) {
            let name = name.trim().to_string();
            if rest.contains("optional = true") {
                optional.push(name);
            } else {
                declared.push(name);
            }
        }
        entry.clear();
    }
    declared.sort();
    optional.sort();
    (declared, optional)
}

// ---------------------------------------------------------------------------
// Half two: ambient authority. "Filesystem" is not a crate name.
// ---------------------------------------------------------------------------

/// The forbidden capability a line of code reaches, if any.
///
/// Matches the module path (`std::fs::read_to_string`), the plain import
/// (`use std::fs;`), the aliased import (`use std::fs as x;`), and brace
/// groups (`use std::{env, fs};`, `use std::{process::Command, …}`).
///
/// **Accepted residual, stated rather than implied:** this is a tripwire, not
/// a proof. A determined author can still reach the filesystem through a
/// re-export or a macro. What it buys is that the ordinary ways in are loud.
fn ambient_hit(code: &str) -> Option<&'static str> {
    for module in FORBIDDEN_STD {
        if code.contains(&format!("std::{module}")) {
            return Some(module);
        }
    }
    let tail = code.split("use std::").nth(1)?;
    for module in FORBIDDEN_STD {
        let mut rest = tail;
        while let Some(at) = rest.find(module) {
            let before_ok = at == 0
                || !rest[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let after_ok = rest[at + module.len()..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            if before_ok && after_ok {
                return Some(module);
            }
            rest = &rest[at + module.len()..];
        }
    }
    None
}

/// Production lines of `source`: string literals and `//` comments blanked,
/// and every `#[cfg(test)]` item skipped by BRACE DEPTH.
///
/// Brace depth, not a latch. A latch that flips on the first `#[cfg(test)]`
/// and never clears skips every later line, so a crate reports clean the
/// moment it contains one test module — which is the failure mode
/// [`the_scanner_sees_past_a_test_module`] exists to catch.
fn production_lines(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut armed = false;
    let mut skip_until: Option<i32> = None;

    for raw in source.lines() {
        let code = blank_strings_and_comments(raw);
        let opens = code.matches('{').count() as i32;
        let closes = code.matches('}').count() as i32;

        if skip_until.is_none() {
            if code.contains("#[cfg(test)]") {
                armed = true;
            } else if armed && opens > 0 {
                skip_until = Some(depth);
                armed = false;
            } else {
                out.push(code.clone());
            }
        }
        depth += opens - closes;
        if let Some(level) = skip_until {
            if depth <= level {
                skip_until = None;
            }
        }
    }
    out
}

/// Blank out `//` comments and the contents of string/char literals, so a
/// `"std::fs"` inside a doc example or an error message is not a hit.
fn blank_strings_and_comments(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(' ');
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(' ');
            }
            '/' if chars.peek() == Some(&'/') => break,
            '\'' => {
                // A char literal ('a', '\n', '"') — never a path. Skip it, but
                // do not confuse it with a lifetime (`'a` followed by no quote).
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Every production line under `src/` that takes ambient authority.
fn ambient_authority(src: &Path) -> (usize, Vec<String>) {
    let mut scanned = 0usize;
    let mut found = Vec::new();
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src/ is readable") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("a source file");
            for line in production_lines(&source) {
                scanned += 1;
                if let Some(hit) = ambient_hit(&line) {
                    found.push(format!("{}: {} [{hit}]", path.display(), line.trim()));
                }
            }
        }
    }
    (scanned, found)
}

#[test]
fn the_crate_takes_no_ambient_authority() {
    let (scanned, found) = ambient_authority(&manifest_dir().join("src"));
    // POSITIVE READ ASSERTION. An absence check fails OPEN: anything that
    // shrinks the scanned text makes it MORE likely to pass. Assert it read
    // something first.
    assert!(
        scanned > 200,
        "the source scan saw only {scanned} production lines — it is not \
         reading this crate, so the absence it reports means nothing"
    );
    assert!(
        found.is_empty(),
        "precedence-ladder takes ambient authority: {found:#?}\nA precedence \
         predicate reads no file, opens no socket, spawns no process, and \
         consults no environment. `from_toml` takes a `&str` for exactly this \
         reason — the consumer owns the read."
    );
}

/// **ANTI-VACUOUS TWIN (a).** The scanner, pointed at a seeded `std::fs` call,
/// must find it. A scanner that reports clean on code it cannot read reports
/// clean on everything.
#[test]
fn the_scanner_would_notice_ambient_authority() {
    for source in [
        "pub fn r() -> String { std::fs::read_to_string(\"/etc/hostname\").unwrap() }",
        "use std::fs;\npub fn r() {}",
        "use std::fs as filesystem;\npub fn r() {}",
        "use std::{collections::BTreeMap, env};\npub fn r() {}",
        "use std::{process::Command, fmt};\npub fn r() {}",
        "pub fn r() { let _ = std::net::TcpStream::connect(\"x\"); }",
    ] {
        let hits: Vec<_> = production_lines(source)
            .iter()
            .filter_map(|l| ambient_hit(l))
            .collect();
        assert!(!hits.is_empty(), "missed ambient authority in:\n{source}");
    }
}

/// **ANTI-VACUOUS TWIN (b).** A mid-file `#[cfg(test)]` must not blind the
/// rest of the file. This is the latch bug: arm on the first test attribute,
/// never clear, and every production line after it goes unscanned — so the
/// crate reports clean the moment it grows a test module. Every file in
/// `src/` has one.
#[test]
fn the_scanner_sees_past_a_test_module() {
    let source = "#[cfg(test)]\nmod tests {\n    fn t() { let _ = 1; }\n}\n\n\
                  pub fn real() -> String { std::fs::read_to_string(\"/x\").unwrap() }\n";
    let lines = production_lines(source);
    assert!(
        lines.iter().any(|l| l.contains("pub fn real")),
        "production code after a test module went unscanned: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("mod tests")),
        "the test module itself was scanned as production: {lines:?}"
    );
    assert_eq!(
        lines.iter().filter_map(|l| ambient_hit(l)).count(),
        1,
        "the seeded std::fs after the test module was missed"
    );
}

/// **ANTI-VACUOUS TWIN (c).** `std::fs` written inside a string literal or a
/// comment is not a call. Without the blanking, this crate's own doc comments
/// (which say "no `std::fs`") would make the guard fail on itself — and the
/// obvious fix would be to weaken the needle.
#[test]
fn the_scanner_ignores_strings_and_comments() {
    for source in [
        "// mentions std::fs in a comment\npub fn r() {}",
        "pub fn r() -> &'static str { \"std::process::Command\" }",
        "/// doc: never use std::env here\npub fn r() {}",
    ] {
        let hits: Vec<_> = production_lines(source)
            .iter()
            .filter_map(|l| ambient_hit(l))
            .collect();
        assert!(hits.is_empty(), "false positive in:\n{source}\n{hits:?}");
    }
}

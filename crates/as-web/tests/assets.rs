//! The embedded web UI subset (D2): every reference resolves, the table matches the
//! directory, the files are the upstream bytes, and every vendor file has its licence.
use as_web::assets::{asset, Asset, ASSETS};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const CORE_JS: &str = "resources/js/airscout.core.js";
const SCOUTBASE_CORE_JS: &str = "resources/js/scoutbase.core.js";
const ORIGIN_LINE: &str = "static _rootURL = window.location.origin;";

fn text(a: &Asset) -> &'static str {
    std::str::from_utf8(a.bytes).unwrap_or_else(|e| panic!("{} not UTF-8: {e}", a.path))
}

fn is_text(a: &Asset) -> bool {
    a.mime.starts_with("text/") || a.mime == "application/javascript" || a.mime == "image/svg+xml"
}

/// The directory part of an asset path, with a trailing "/" (or "").
fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..=i])
}

/// Resolves a reference like a browser: "/" starts at the root, ".." at the root stays there.
fn resolve(base_dir: &str, reference: &str) -> String {
    let joined = match reference.strip_prefix('/') {
        Some(rest) => rest.to_string(),
        None => format!("{base_dir}{reference}"),
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// The reference without its "?…" or "#…" suffix, or `None` for a reference that
/// names no local file.
fn local_ref(r: &str) -> Option<&str> {
    let r = r.trim();
    let lower = r.to_ascii_lowercase();
    if r.is_empty()
        || r.starts_with('#')
        || ["data:", "http:", "https:", "javascript:"]
            .iter()
            .any(|p| lower.starts_with(p))
    {
        return None;
    }
    let end = r.find(['?', '#']).unwrap_or(r.len());
    Some(&r[..end])
}

/// The quoted value that starts at `s` (single or double quotes).
fn quoted(s: &str) -> Option<&str> {
    let q = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &s[1..];
    rest.find(q).map(|end| &rest[..end])
}

/// The tags of an HTML page, as the text between "<" and ">".
fn tags(html: &str) -> impl Iterator<Item = &str> {
    html.split('<').skip(1).filter_map(|t| t.split('>').next())
}

/// The value of attribute `name` in a tag body, matched after white space.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pat = format!("{name}=");
    let mut from = 0;
    while let Some(i) = tag[from..].find(&pat) {
        let at = from + i;
        let before = tag[..at].chars().last();
        if before.is_some_and(char::is_whitespace) {
            return quoted(&tag[at + pat.len()..]);
        }
        from = at + pat.len();
    }
    None
}

/// The references of a page (subset rule): `src`, `href`, `xlink:href` and the
/// `msapplication-TileImage` `content`. The `<base>` element's `href` is skipped.
fn html_refs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in tags(html) {
        let name = tag
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if name == "base" {
            continue;
        }
        for a in ["src", "href", "xlink:href"] {
            out.extend(attr(tag, a).map(str::to_string));
        }
        if attr(tag, "name") == Some("msapplication-TileImage") {
            out.extend(attr(tag, "content").map(str::to_string));
        }
    }
    out
}

/// Every `url(...)` of a style sheet, quotes removed.
fn css_refs(css: &str) -> Vec<String> {
    css.split("url(")
        .skip(1)
        .filter_map(|s| s.split(')').next())
        .map(|s| s.trim().trim_matches(['"', '\'']).to_string())
        .collect()
}

/// The `<base href>` directory of a page, joined to the page's own directory.
fn base_dir(page: &str, html: &str) -> String {
    let base = tags(html)
        .find(|t| t.split_whitespace().next() == Some("base"))
        .and_then(|t| attr(t, "href"));
    match base {
        Some(b) => {
            let d = resolve(dir_of(page), b);
            if d.is_empty() {
                d
            } else {
                format!("{d}/")
            }
        }
        None => dir_of(page).to_string(),
    }
}

#[test]
fn every_reference_resolves() {
    let mut checked = 0;
    for page in ["index.html", "dist/index.html"] {
        let a = asset(page).unwrap_or_else(|| panic!("{page} missing"));
        let html = text(a);
        let base = base_dir(page, html);
        let refs = html_refs(html);
        assert!(!refs.is_empty(), "{page}: no references found");
        for r in refs.iter().filter_map(|r| local_ref(r)) {
            let p = resolve(&base, r);
            assert!(asset(&p).is_some(), "{page}: {r} -> {p} not embedded");
            checked += 1;
        }
    }
    for a in ASSETS.iter().filter(|a| a.path.ends_with(".css")) {
        for r in css_refs(text(a)).iter().filter_map(|r| local_ref(r)) {
            let p = resolve(dir_of(a.path), r);
            assert!(asset(&p).is_some(), "{}: {r} -> {p} not embedded", a.path);
            checked += 1;
        }
    }
    // The two pages and their style sheets hold 42 local references (some to the
    // same file); fewer means the scanner missed some.
    assert_eq!(checked, 42, "references checked");
}

#[test]
fn reference_scanner_reads_both_quote_styles() {
    let html = "<base href=\"./\"><a href='x.html'><use xlink:href=\"s.svg#i\">\
                <meta name=\"msapplication-TileImage\" content=\"t.png\"><img src=\"i.png\" data-src=\"no\">";
    assert_eq!(html_refs(html), ["x.html", "s.svg#i", "t.png", "i.png"]);
    assert_eq!(base_dir("dist/index.html", html), "dist/");
    assert_eq!(
        resolve("dist/", "/../../resources/js/a.js"),
        "resources/js/a.js"
    );
    assert_eq!(
        css_refs("a{b:url( \"images/x.png\" )} c{d:url(#default#VML)}"),
        ["images/x.png", "#default#VML"]
    );
    assert_eq!(local_ref("#default#VML"), None);
    assert_eq!(local_ref("javascript:play_pause();"), None);
    assert_eq!(local_ref("a.svg#full"), Some("a.svg"));
}

fn files_under(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            files_under(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap();
            let parts: Vec<_> = rel.iter().map(|s| s.to_str().unwrap()).collect();
            out.insert(parts.join("/"));
        }
    }
}

fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

#[test]
fn table_matches_directory() {
    let mut on_disk = BTreeSet::new();
    files_under(&assets_dir(), &assets_dir(), &mut on_disk);
    let table: BTreeSet<String> = ASSETS.iter().map(|a| a.path.to_string()).collect();
    assert_eq!(table.len(), ASSETS.len(), "duplicate paths in ASSETS");
    assert_eq!(table, on_disk);
    for a in ASSETS {
        let disk = std::fs::read(assets_dir().join(a.path)).unwrap();
        assert_eq!(disk, a.bytes, "{} bytes", a.path);
        assert!(std::ptr::eq(asset(a.path).unwrap(), a), "{} lookup", a.path);
    }
}

#[test]
fn mime_types_by_extension() {
    let expect = [
        (".html", "text/html"),
        (".js", "application/javascript"),
        (".css", "text/css"),
        (".png", "image/png"),
        (".svg", "image/svg+xml"),
        (".ico", "image/x-icon"),
        (".txt", "text/plain"),
    ];
    for a in ASSETS {
        let (_, mime) = expect
            .iter()
            .find(|(ext, _)| a.path.ends_with(ext))
            .unwrap_or_else(|| panic!("{}: unexpected extension", a.path));
        assert_eq!(a.mime, *mime, "{}", a.path);
    }
}

#[test]
fn core_js_uses_page_origin() {
    let js = text(asset(CORE_JS).unwrap());
    assert!(js.contains(ORIGIN_LINE));
    assert!(!js.contains("_rootURL = \"http"));
}

#[test]
fn finding_16_web_unit_converter_uses_correct_mile_factors() {
    let js = text(asset("resources/js/scoutbase.core.js").unwrap());
    assert!(js.contains("return km / 1.609344;"));
    assert!(js.contains("return mi * 1.609344;"));
}

/// Every dotted quad in `s` with four octets of at most 255.
fn dotted_quads(s: &str) -> Vec<[u8; 4]> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let starts =
            b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_digit() || b[i - 1] == b'.'));
        if !starts {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut octets = Vec::new();
        loop {
            let k = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j == k || j - k > 3 {
                break;
            }
            octets.push(s[k..j].parse::<u16>().unwrap());
            if octets.len() == 4 || j >= b.len() || b[j] != b'.' {
                break;
            }
            j += 1;
        }
        let ends = j >= b.len()
            || !(b[j].is_ascii_digit()
                || (b[j] == b'.' && b.get(j + 1).is_some_and(u8::is_ascii_digit)));
        if octets.len() == 4 && ends && octets.iter().all(|o| *o <= 255) {
            out.push([
                octets[0] as u8,
                octets[1] as u8,
                octets[2] as u8,
                octets[3] as u8,
            ]);
        }
        i = j.max(i + 1);
    }
    out
}

fn is_private(q: [u8; 4]) -> bool {
    q[0] == 10 || (q[0] == 172 && (16..=31).contains(&q[1])) || (q[0] == 192 && q[1] == 168)
}

#[test]
fn private_ipv4_scanner_finds_ranges() {
    // Built at run time, so no private address is written in this file.
    let quad = |a: u8, b: u8| format!("{a}.{b}.7.9");
    for (a, b, private) in [
        (10, 0, true),
        (172, 16, true),
        (172, 31, true),
        (172, 32, false),
        (192, 168, true),
        (192, 169, false),
        (8, 8, false),
    ] {
        let s = format!("x = \"http://{}:9880\";", quad(a, b));
        let found = dotted_quads(&s);
        assert_eq!(found.len(), 1, "{s}");
        assert_eq!(is_private(found[0]), private, "{s}");
    }
    assert!(
        dotted_quads(&format!("v{}.1", quad(10, 0))).is_empty(),
        "five parts"
    );
    assert!(dotted_quads("1.2.3").is_empty());
    assert!(dotted_quads(&quad(1, 0).replace('7', "700")).is_empty());
}

#[test]
fn no_private_ipv4_in_assets() {
    for a in ASSETS.iter().filter(|a| is_text(a)) {
        let s = String::from_utf8_lossy(a.bytes);
        let bad: Vec<_> = dotted_quads(&s)
            .into_iter()
            .filter(|q| is_private(*q))
            .collect();
        assert!(bad.is_empty(), "{}: private IPv4 address", a.path);
    }
}

/// The entries of `THIRD-PARTY.txt`: (component, licence texts, files).
fn third_party() -> Vec<(String, Vec<String>, Vec<String>)> {
    let t = text(asset("LICENSES/THIRD-PARTY.txt").expect("THIRD-PARTY.txt missing"));
    let mut out: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    let mut in_files = false;
    for line in t.lines() {
        if let Some(c) = line.strip_prefix("Component: ") {
            out.push((c.trim().to_string(), Vec::new(), Vec::new()));
            in_files = false;
        } else if let Some(l) = line.strip_prefix("Licence text: ") {
            let e = out.last_mut().expect("Licence text before Component");
            e.1.extend(l.split(',').map(|s| s.trim().to_string()));
        } else if line.starts_with("Files:") {
            in_files = true;
        } else if in_files && line.starts_with("  ") && !line.trim().is_empty() {
            out.last_mut().unwrap().2.push(line.trim().to_string());
        } else {
            in_files = false;
        }
    }
    out
}

#[test]
fn licences_listed() {
    let entries = third_party();
    assert!(entries.len() >= 14, "{} entries", entries.len());
    let mut listed = BTreeSet::new();
    for (component, licences, files) in &entries {
        assert!(!licences.is_empty(), "{component}: no licence text");
        assert!(!files.is_empty(), "{component}: no files");
        for l in licences {
            assert!(
                l.starts_with("LICENSES/") && asset(l).is_some(),
                "{component}: {l} missing"
            );
        }
        for f in files {
            assert!(asset(f).is_some(), "{component}: {f} not embedded");
            assert!(listed.insert(f.clone()), "{f} listed twice");
        }
    }
    // Every embedded file except the licence texts belongs to exactly one component,
    // so every vendor directory of the subset has an entry.
    for a in ASSETS.iter().filter(|a| !a.path.starts_with("LICENSES/")) {
        assert!(
            listed.contains(a.path),
            "{} has no THIRD-PARTY.txt entry",
            a.path
        );
    }
    let vendor_dirs: BTreeSet<&str> = ASSETS
        .iter()
        .filter_map(|a| a.path.strip_prefix("dist/vendors/"))
        .map(|p| {
            let n = if p.starts_with('@') { 2 } else { 1 };
            let end = p.match_indices('/').nth(n - 1).map_or(p.len(), |(i, _)| i);
            &p[..end]
        })
        .collect();
    assert_eq!(vendor_dirs.len(), 10, "{vendor_dirs:?}");
    for d in vendor_dirs {
        let prefix = format!("dist/vendors/{d}/");
        assert!(
            listed.iter().any(|f| f.starts_with(&prefix)),
            "{d}: no entry"
        );
    }
}

fn upstream_wwwroot() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/upstream/AirScout/AirScout/wwwroot")
}

#[test]
fn unchanged_from_upstream() {
    let root = upstream_wwwroot();
    if !root.is_dir() {
        eprintln!("target/upstream missing; skipped");
        return;
    }
    for a in ASSETS.iter().filter(|a| !a.path.starts_with("LICENSES/")) {
        let orig = std::fs::read(root.join(a.path)).unwrap_or_else(|e| panic!("{}: {e}", a.path));
        if a.path != CORE_JS && a.path != SCOUTBASE_CORE_JS {
            assert!(orig == a.bytes, "{} differs from upstream", a.path);
            continue;
        }
        if a.path == SCOUTBASE_CORE_JS {
            let ours: Vec<&[u8]> = a.bytes.split(|b| *b == b'\n').collect();
            let theirs: Vec<&[u8]> = orig.split(|b| *b == b'\n').collect();
            assert_eq!(ours.len(), theirs.len(), "line count");
            let changed: Vec<usize> = (0..ours.len())
                .filter(|i| ours[*i] != theirs[*i])
                .map(|i| i + 1)
                .collect();
            assert_eq!(changed, [1093, 1098]);
            continue;
        }
        let ours: Vec<&[u8]> = a.bytes.split(|b| *b == b'\n').collect();
        let theirs: Vec<&[u8]> = orig.split(|b| *b == b'\n').collect();
        assert_eq!(ours.len(), theirs.len(), "line count");
        let changed: Vec<usize> = (0..ours.len())
            .filter(|i| ours[*i] != theirs[*i])
            .map(|i| i + 1)
            .collect();
        assert_eq!(changed, [563]);
        let line = std::str::from_utf8(ours[562]).unwrap();
        assert_eq!(line.trim(), ORIGIN_LINE);
        let indent = |l: &[u8]| l.iter().take_while(|b| **b == b' ').count();
        assert_eq!(indent(ours[562]), indent(theirs[562]));
        assert_eq!(ours[562].ends_with(b"\r"), theirs[562].ends_with(b"\r"));
    }
}

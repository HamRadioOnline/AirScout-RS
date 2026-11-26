//! Query parsing as the C# routes do it (`Webserver.cs:514-515`): `ToUpper()`, then
//! `HttpUtility.ParseQueryString`; numbers by `double.TryParse(.., NumberStyles.Float,
//! Invariant)`; the static path by `Uri.UnescapeDataString`.
use as_web::query::{parse_float, unescape_data_string, Query};

fn get(url: &str, key: &str) -> Option<String> {
    Query::from_raw_url(url).get(key)
}

#[test]
fn query_upper_cases_and_decodes() {
    let q = Query::from_raw_url("/location.json?call=te1st&loc=jo50aa");
    assert_eq!(q.get("CALL").as_deref(), Some("TE1ST"));
    assert_eq!(q.get("LOC").as_deref(), Some("JO50AA"));
    // `ToUpper()` runs before decoding, so an escape still gives a lower-case letter.
    assert_eq!(
        get("/location.json?call=%61b", "CALL").as_deref(),
        Some("aB")
    );
    // `NameValueCollection.Get` joins several values with ",".
    assert_eq!(get("/x?a=1&a=2", "A").as_deref(), Some("1,2"));
    assert_eq!(get("/x?a=&a=2", "A").as_deref(), Some(",2"));
    // A part without "=" has a null key and is never found.
    assert_eq!(get("/location.json?call", "CALL"), None);
    // `+` is a space in a query value.
    assert_eq!(get("/x?x=a+b", "X").as_deref(), Some("A B"));
    // Only the first "=" splits.
    assert_eq!(get("/x?x=a=b", "X").as_deref(), Some("A=B"));
}

#[test]
fn query_edge_cases() {
    // Invalid or incomplete escapes stay literal.
    assert_eq!(get("/x?x=%zz%", "X").as_deref(), Some("%ZZ%"));
    assert_eq!(get("/x?x=%", "X").as_deref(), Some("%"));
    assert_eq!(get("/x?x=%4", "X").as_deref(), Some("%4"));
    // A valid escape of an incomplete UTF-8 sequence decodes lossily (`HttpUtility.UrlDecode`).
    assert_eq!(get("/x?x=%C3", "X").as_deref(), Some("\u{FFFD}"));
    assert_eq!(get("/x?x=%C3%A4", "X").as_deref(), Some("ä"));
    assert_eq!(get("/x?x=ä+ö%", "X").as_deref(), Some("Ä Ö%"));
    // Keys are decoded too, and `HttpValueCollection` compares them ignoring case.
    assert_eq!(get("/x?%61=1", "A").as_deref(), Some("1"));
    // `ParseQueryString` drops one more leading "?".
    assert_eq!(get("/x??x=1", "X").as_deref(), Some("1"));
    // Only the text after the first "?" counts.
    assert_eq!(get("/x?x=1?y=2", "X").as_deref(), Some("1?Y=2"));

    // A 100 kB query parses without panic, in proportion to its size.
    let long = format!("/x?a={}&b={}", "%".repeat(50_000), "%C3".repeat(20_000));
    let q = Query::from_raw_url(&long);
    assert_eq!(q.get("A").map(|v| v.len()), Some(50_000));
    assert_eq!(q.get("B").map(|v| v.chars().count()), Some(20_000));
    let many = "&k=v".repeat(25_000);
    assert_eq!(
        get(&format!("/x?{many}"), "K").map(|v| v.len()),
        Some(25_000 * 2 - 1)
    );

    // No "?" gives an empty query.
    let q = Query::from_raw_url("/bands.json");
    assert!(q.is_empty());
    assert_eq!(q.get(""), None);
    assert!(Query::from_raw_url("/bands.json?").is_empty());
}

#[test]
fn unescape_data_string_rules() {
    // `Uri.UnescapeDataString`: only escapes that form valid UTF-8 are decoded.
    assert_eq!(unescape_data_string("/%C3"), "/%C3");
    assert_eq!(unescape_data_string("/%c3%28"), "/%c3(");
    assert_eq!(unescape_data_string("/%C3%A4.html"), "/ä.html");
    assert_eq!(unescape_data_string("/%2e%2e/%2E%2E/x"), "/../../x");
    assert_eq!(unescape_data_string("/%zz%"), "/%zz%");
    assert_eq!(unescape_data_string("%"), "%");
    assert_eq!(unescape_data_string("/a+b"), "/a+b");
    assert_eq!(unescape_data_string("/%00"), "/\0");
    assert_eq!(unescape_data_string("/%F0%9F%98%80%FF"), "/\u{1F600}%FF");
    assert_eq!(unescape_data_string("/ä%zzö%41"), "/ä%zzöA");
    let long = "%E2%82".repeat(20_000);
    assert_eq!(unescape_data_string(&long), long);
}

#[test]
fn parse_float_rules() {
    assert_eq!(parse_float("50.5"), Some(50.5));
    assert_eq!(parse_float(" -1.5E+02 "), Some(-150.0));
    assert_eq!(parse_float("1e5"), Some(100_000.0));
    assert_eq!(parse_float(".5"), Some(0.5));
    assert_eq!(parse_float("5."), Some(5.0));
    assert_eq!(parse_float("+7"), Some(7.0));
    assert_eq!(parse_float("\t8\n"), Some(8.0));
    for bad in [
        "",
        ".",
        "1,000",
        "NAN",
        "INFINITY",
        "-INFINITY",
        "0x10",
        "1e",
        "1e+",
        "--1",
        "+-1",
        "1 2",
        "- 1",
        "1.2.3",
        "e5",
        // .NET Framework `TryParse` fails on overflow instead of giving infinity.
        "1e400",
    ] {
        assert_eq!(parse_float(bad), None, "{bad:?}");
    }
}

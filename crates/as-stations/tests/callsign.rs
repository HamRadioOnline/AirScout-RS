use as_golden::deltas::DeltaManifest;
use as_stations::callsign;
use serde_json::Value;

#[test]
fn callsign_matches_original() {
    let g = as_golden::load("callsign.json");
    assert_eq!(g.cases.len(), 180);
    let mut deltas =
        DeltaManifest::load_from("docs/logic-fixes-area3/golden-deltas.tsv", "callsign");
    for (index, c) in g.cases.iter().enumerate() {
        let call = c.s("call");
        // The normalized calls share case 1's base, whose German prefix is DL.
        // The leading DL in DL/AB1 is a country prefix and case 163 is its base.
        let reference = match index {
            19 | 22 => g.cases[1].result.clone(),
            23 => Value::from("DL"),
            57 => Value::from(true),
            58 => g.cases[163].result.clone(),
            _ => c.result.clone(),
        };
        let expected = deltas.expected(index, "result", &c.result, reference, 0.0, "#14");
        let actual = match c.func.as_str() {
            "check" => Value::from(callsign::check(&call)),
            "cut" => Value::from(callsign::cut(&call)),
            "prefix" => Value::from(callsign::prefix(&call)),
            other => panic!("unhandled {other}"),
        };
        assert_eq!(actual, expected, "{c:?}");
    }
    deltas.finish();
}

#[test]
fn brief_examples() {
    assert!(callsign::check("DL2ALF"));
    assert_eq!(callsign::cut("DL2ALF/P"), "DL2ALF");
    assert!(!callsign::check(""));
    assert_eq!(callsign::cut("PA/DL2ALF/M"), "DL2ALF");
}

#[test]
fn finding_14_normalizes_before_cutting() {
    assert_eq!(callsign::cut("  te1st/p  "), "TE1ST");
}

#[test]
fn finding_14_single_slash_can_have_a_short_base() {
    assert_eq!(callsign::cut("DL/A1B"), "A1B");
    assert!(callsign::check("DL/A1B"));
    assert_eq!(callsign::cut("DL/TE1ST/P"), "TE1ST");
    assert!(!callsign::check("DL//TE1ST"));
}

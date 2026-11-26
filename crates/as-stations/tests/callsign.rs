use as_stations::callsign;

#[test]
fn callsign_matches_original() {
    let g = as_golden::load("callsign.json");
    assert_eq!(g.cases.len(), 180);
    for c in &g.cases {
        let call = c.s("call");
        match c.func.as_str() {
            "check" => assert_eq!(callsign::check(&call), c.result.as_bool().unwrap(), "{c:?}"),
            "cut" => assert_eq!(callsign::cut(&call), c.result.as_str().unwrap(), "{c:?}"),
            "prefix" => assert_eq!(callsign::prefix(&call), c.result.as_str().unwrap(), "{c:?}"),
            other => panic!("unhandled {other}"),
        }
    }
}

#[test]
fn brief_examples() {
    assert!(callsign::check("DL2ALF"));
    assert_eq!(callsign::cut("DL2ALF/P"), "DL2ALF");
    assert!(!callsign::check(""));
    assert_eq!(callsign::cut("PA/DL2ALF/M"), "DL2ALF");
}

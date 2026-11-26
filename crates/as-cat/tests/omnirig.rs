#[path = "common/fake_omni.rs"]
mod fake_omni;

use as_cat::omnirig::*;
use as_cat::{Doppler, DopplerSession, OmniOpen, OmniProp, OmniRig, Rig, RigMode, RigRit};
use as_cat::{RigSplit, RigStatus};
use fake_omni::FakeOmni;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const F: i64 = 144_300_000;
const ALL_MODES: [i32; 8] = [
    PM_CW_L, PM_CW_U, PM_SSB_L, PM_SSB_U, PM_DIG_L, PM_DIG_U, PM_AM, PM_FM,
];

fn opener(fake: &FakeOmni) -> OmniOpen {
    let fake = fake.clone();
    Box::new(move || Ok(Box::new(fake.clone())))
}

fn rig_with(fake: &FakeOmni, n: u8) -> OmniRig {
    OmniRig::new(opener(fake), n)
}

fn writeable(fake: &mut FakeOmni, params: &[i32]) {
    fake.writeable.extend(params.iter().copied());
}

#[test]
fn status_mapping() {
    for (st, want) in [
        (0, RigStatus::NoRig),
        (1, RigStatus::NoRig),
        (2, RigStatus::NoPort),
        (3, RigStatus::Offline),
        (4, RigStatus::Online),
        (7, RigStatus::Offline),
        (-1, RigStatus::Offline),
    ] {
        let mut fake = FakeOmni::new(2);
        fake.status = st;
        assert_eq!(
            rig_with(&fake, 1).get_status(Doppler::None),
            want,
            "status {st}"
        );
    }
}

#[test]
fn every_active_doppler_strategy_needs_split_and_vfoequal() {
    let mut fake = FakeOmni::new(2);
    fake.readable.extend([PM_SPLITOFF, PM_SPLITON]);
    writeable(&mut fake, &[PM_SPLITOFF, PM_SPLITON]);
    for s in [Doppler::A, Doppler::B, Doppler::C, Doppler::D] {
        assert_eq!(
            rig_with(&fake, 1).get_status(s),
            RigStatus::NotSuitable,
            "{s:?}"
        );
    }
    fake.writeable.insert(PM_VFOEQUAL);
    for s in [Doppler::A, Doppler::B, Doppler::C, Doppler::D] {
        assert_eq!(rig_with(&fake, 1).get_status(s), RigStatus::Online, "{s:?}");
    }
}

#[test]
fn open_error_is_nocat_with_text() {
    let tries = Arc::new(AtomicUsize::new(0));
    let t = tries.clone();
    let open: OmniOpen = Box::new(move || {
        t.fetch_add(1, Ordering::SeqCst);
        Err("no class".to_string())
    });
    let mut rig = OmniRig::new(open, 1);
    assert_eq!(rig.ident().cat_version, "OmniRig");
    assert_eq!(rig.get_status(Doppler::None), RigStatus::NoCat);
    assert_eq!(
        rig.take_error().as_deref(),
        Some("Error while initializing OmniRig: no class")
    );
    assert_eq!(rig.take_error(), None);
    assert_eq!(rig.get_status(Doppler::None), RigStatus::NoCat);
    assert_eq!(tries.load(Ordering::SeqCst), 2);
}

#[test]
fn rig3_on_v1_engine_is_norig() {
    let fake = FakeOmni::new(2);
    let mut rig = rig_with(&fake, 3);
    assert_eq!(rig.get_status(Doppler::None), RigStatus::NoRig);
    assert_eq!(
        rig.take_error().as_deref(),
        Some("OmniRig V1.19 supports Rig 1 and Rig 2 only.")
    );
    assert_eq!(rig.ident().cat_version, "OmniRig V1.19");
}

#[test]
fn v2_engine_has_four_rigs() {
    let fake = FakeOmni::new(4);
    let mut rig = rig_with(&fake, 4);
    assert_eq!(rig.get_status(Doppler::None), RigStatus::Online);
    let id = rig.ident();
    assert_eq!(id.cat_version, "OmniRig V2.x");
    assert_eq!(id.rig_type, "[OmniRig] Rig 4");
}

#[test]
fn com_error_on_status_is_none() {
    let mut fake = FakeOmni::new(2);
    fake.fail.insert("Status");
    assert_eq!(
        rig_with(&fake, 1).get_status(Doppler::None),
        RigStatus::None
    );
}

#[test]
fn getters_follow_csharp() {
    let mut fake = FakeOmni::new(2);
    fake.props.insert(OmniProp::Split, PM_SPLITON);
    fake.props.insert(OmniProp::FreqA, 144_300_312);
    fake.props.insert(OmniProp::FreqB, 144_300_000);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert_eq!(rig.get_split(), RigSplit::SplitOn);
    assert_eq!(rig.get_rx_frequency(), 144_300_312);
    assert_eq!(rig.get_tx_frequency(), 144_300_000);
    assert_eq!(rig.get_rit(), RigRit::RitOff);
    assert_eq!(rig.get_mode(), RigMode::Usb);

    let mut fake = FakeOmni::new(2);
    fake.props.insert(OmniProp::Freq, 0);
    fake.props.insert(OmniProp::FreqA, F as i32);
    fake.props.insert(OmniProp::Mode, 0x1234);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert_eq!(rig.get_rx_frequency(), F);
    assert_eq!(rig.get_tx_frequency(), F);
    assert_eq!(rig.get_mode(), RigMode::None);
    fake.fail.insert("Freq");
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert_eq!(rig.get_rx_frequency(), 0);
}

#[test]
fn set_frequency_writes_when_writeable() {
    let mut fake = FakeOmni::new(2);
    writeable(&mut fake, &[PM_FREQ, PM_FREQA, PM_FREQB]);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert!(rig.set_rx_frequency(F));
    assert_eq!(fake.log(), ["Freq=144300000"]);
    fake.clear_log();
    assert!(rig.set_tx_frequency(F));
    assert_eq!(fake.log(), ["Freq=144300000"]);
}

#[test]
fn set_frequency_false_when_not_writeable() {
    let fake = FakeOmni::new(2);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert!(!rig.set_rx_frequency(F));
    assert!(!rig.set_tx_frequency(F));
    assert!(fake.log().is_empty());
}

#[test]
fn split_mode_ts570() {
    let mut fake = FakeOmni::new(2);
    writeable(&mut fake, &[PM_FREQA, PM_FREQB, PM_VFOAB, PM_SPLITON]);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert!(rig.set_split_mode(144_300_312, F, RigMode::None));
    assert_eq!(
        fake.log(),
        [
            "Vfo=PM_VFOAB",
            "FreqA=144300312",
            "FreqB=144300000",
            "Split=PM_SPLITON"
        ]
    );
}

#[test]
fn simplex_freq_only() {
    let mut fake = FakeOmni::new(2);
    writeable(&mut fake, &[PM_FREQ]);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert!(rig.set_simplex_mode(F, RigMode::None));
    assert_eq!(fake.log(), ["Freq=144300000"]);
}

#[test]
fn mode_write_is_the_pm_value_not_the_rigmode_number() {
    let mut fake = FakeOmni::new(2);
    writeable(&mut fake, &[PM_FREQ]);
    writeable(&mut fake, &ALL_MODES);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert!(rig.set_simplex_mode(F, RigMode::Lsb));
    assert_eq!(fake.log(), ["Freq=144300000", "Mode=PM_SSB_L"]);
    fake.clear_log();
    assert!(rig.set_mode(RigMode::Fm));
    assert_eq!(fake.log(), ["Mode=PM_FM"]);
}

#[test]
fn limits() {
    let mut fake = FakeOmni::new(2);
    writeable(
        &mut fake,
        &[PM_FREQ, PM_FREQA, PM_FREQB, PM_VFOAB, PM_SPLITON],
    );
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert!(!rig.set_split_mode(3_000_000_000, F, RigMode::None));
    assert!(!rig.set_split_mode(F, 0, RigMode::None));
    assert!(!rig.set_simplex_mode(0, RigMode::Usb));
    assert!(!rig.set_simplex_mode(i64::from(i32::MAX) + 1, RigMode::Usb));
    assert!(!rig.set_mode(RigMode::Other));
    assert!(!rig.set_split(RigSplit::None));
    assert!(!rig.set_rit(RigRit::None));
    assert!(fake.log().is_empty());
}

#[test]
fn com_error_in_a_sequence_is_false() {
    let mut fake = FakeOmni::new(2);
    writeable(&mut fake, &[PM_FREQ]);
    fake.fail.insert("Freq");
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    assert!(!rig.set_simplex_mode(F, RigMode::None));
}

#[test]
fn golden_doppler_session_ic746() {
    let mut fake = FakeOmni::new(2);
    writeable(&mut fake, &[PM_FREQ, PM_VFOEQUAL, PM_SPLITON, PM_SPLITOFF]);
    writeable(&mut fake, &ALL_MODES);
    fake.readable.extend([PM_SPLITON, PM_SPLITOFF]);
    let mut rig = rig_with(&fake, 1);
    let mut d = DopplerSession::default();
    assert!(d.enter(&mut rig, Doppler::A));
    assert_eq!(
        fake.log(),
        [
            "Mode=PM_SSB_U",
            "Vfo=PM_VFOEQUAL",
            "Freq=144300000",
            "Mode=PM_SSB_U",
            "Vfo=PM_VFOEQUAL",
            "Freq=144300000",
            "Split=PM_SPLITON"
        ]
    );
    fake.clear_log();
    d.leave(&mut rig);
    assert_eq!(
        fake.log(),
        [
            "Mode=PM_SSB_U",
            "Split=PM_SPLITOFF",
            "Rit=PM_RITOFF",
            "Freq=144300000",
            "Freq=144300000",
            "Freq=144300000",
            "Vfo=PM_VFOEQUAL",
            "Split=PM_SPLITOFF",
            "Mode=PM_SSB_U"
        ]
    );
}

#[test]
fn ident_after_open_carries_version_and_rig() {
    let fake = FakeOmni::new(4);
    let mut rig = rig_with(&fake, 3);
    rig.get_status(Doppler::None);
    let id = rig.ident();
    assert_eq!(id.cat_version, "OmniRig V2.x");
    assert_eq!(id.rig_type, "[OmniRig] Rig 3");
}

fn online_rig(params: &[i32]) -> (FakeOmni, OmniRig) {
    let mut fake = FakeOmni::new(2);
    writeable(&mut fake, params);
    let mut rig = rig_with(&fake, 1);
    rig.get_status(Doppler::None);
    (fake, rig)
}

/// `IntSetSimplexMode` (OmniRigX.cs:984-1022): the first matching branch wins.
#[test]
fn simplex_branches_in_csharp_order() {
    let f = "144300000";
    let cases: Vec<(&str, Vec<i32>, Vec<String>)> = vec![
        (
            "1 FREQA+VFOAA",
            vec![PM_FREQA, PM_VFOAA],
            vec!["Vfo=PM_VFOAA".into(), format!("FreqA={f}")],
        ),
        (
            "2 FREQA+VFOA+SPLITOFF",
            vec![PM_FREQA, PM_VFOA, PM_SPLITOFF],
            vec![
                "Vfo=PM_VFOA".into(),
                format!("FreqA={f}"),
                "Split=PM_SPLITOFF".into(),
            ],
        ),
        (
            "3 FREQ+VFOA+VFOB",
            vec![PM_FREQ, PM_VFOA, PM_VFOB],
            vec![
                "Vfo=PM_VFOB".into(),
                format!("Freq={f}"),
                "Vfo=PM_VFOA".into(),
                format!("Freq={f}"),
            ],
        ),
        (
            "4 FREQ+VFOEQUAL",
            vec![PM_FREQ, PM_VFOEQUAL],
            vec![format!("Freq={f}"), "Vfo=PM_VFOEQUAL".into()],
        ),
        (
            "5 FREQ+VFOSWAP",
            vec![PM_FREQ, PM_VFOSWAP],
            vec![
                "Vfo=PM_VFOSWAP".into(),
                format!("Freq={f}"),
                "Vfo=PM_VFOSWAP".into(),
                format!("Freq={f}"),
            ],
        ),
        (
            "6 FREQ+FREQB",
            vec![PM_FREQ, PM_FREQB],
            vec![format!("Freq={f}"), format!("FreqB={f}")],
        ),
        ("7 FREQ", vec![PM_FREQ], vec![format!("Freq={f}")]),
        (
            "1 beats 4",
            vec![PM_FREQA, PM_VFOAA, PM_FREQ, PM_VFOEQUAL],
            vec!["Vfo=PM_VFOAA".into(), format!("FreqA={f}")],
        ),
        (
            "2 beats 3",
            vec![PM_FREQA, PM_VFOA, PM_SPLITOFF, PM_FREQ, PM_VFOB],
            vec![
                "Vfo=PM_VFOA".into(),
                format!("FreqA={f}"),
                "Split=PM_SPLITOFF".into(),
            ],
        ),
        (
            "3 beats 4, 5, 6",
            vec![PM_FREQ, PM_VFOA, PM_VFOB, PM_VFOEQUAL, PM_VFOSWAP, PM_FREQB],
            vec![
                "Vfo=PM_VFOB".into(),
                format!("Freq={f}"),
                "Vfo=PM_VFOA".into(),
                format!("Freq={f}"),
            ],
        ),
        (
            "4 beats 5, 6",
            vec![PM_FREQ, PM_VFOEQUAL, PM_VFOSWAP, PM_FREQB],
            vec![format!("Freq={f}"), "Vfo=PM_VFOEQUAL".into()],
        ),
        (
            "5 beats 6",
            vec![PM_FREQ, PM_VFOSWAP, PM_FREQB],
            vec![
                "Vfo=PM_VFOSWAP".into(),
                format!("Freq={f}"),
                "Vfo=PM_VFOSWAP".into(),
                format!("Freq={f}"),
            ],
        ),
        (
            "6 needs VFOA not writeable",
            vec![PM_FREQ, PM_VFOA, PM_FREQB],
            vec![format!("Freq={f}")],
        ),
    ];
    for (name, params, want) in cases {
        let (fake, mut rig) = online_rig(&params);
        assert!(rig.set_simplex_mode(F, RigMode::None), "{name}");
        assert_eq!(fake.log(), want, "{name}");
    }
}

/// `IntSetSplitMode` (OmniRigX.cs:1128-1206) with rx 144300312 and tx 144300000.
#[test]
fn split_branches_in_csharp_order() {
    let (rx, tx) = ("144300312", "144300000");
    let ts570 = || -> Vec<String> {
        vec![
            "Vfo=PM_VFOAB".into(),
            format!("FreqA={rx}"),
            format!("FreqB={tx}"),
        ]
    };
    let ic746 = || -> Vec<String> {
        vec![
            format!("Freq={tx}"),
            "Vfo=PM_VFOEQUAL".into(),
            format!("Freq={rx}"),
        ]
    };
    let ft100 = || -> Vec<String> {
        vec![
            "Vfo=PM_VFOB".into(),
            format!("Freq={tx}"),
            "Vfo=PM_VFOA".into(),
            format!("Freq={rx}"),
        ]
    };
    let ft817 = || -> Vec<String> {
        vec![
            "Vfo=PM_VFOSWAP".into(),
            format!("Freq={tx}"),
            "Vfo=PM_VFOSWAP".into(),
            format!("Freq={rx}"),
        ]
    };
    let ft1000 = || -> Vec<String> {
        vec![
            "Vfo=PM_VFOA".into(),
            format!("FreqA={rx}"),
            format!("FreqB={tx}"),
        ]
    };
    let cases: Vec<(&str, Vec<i32>, Vec<String>)> = vec![
        ("1 TS-570", vec![PM_FREQA, PM_FREQB, PM_VFOAB], ts570()),
        ("2 IC-746", vec![PM_FREQ, PM_VFOEQUAL], ic746()),
        ("3 FT-100D", vec![PM_VFOB, PM_FREQ, PM_VFOA], ft100()),
        ("4 FT-817", vec![PM_FREQ, PM_VFOSWAP], ft817()),
        ("5 FT-1000MP", vec![PM_FREQA, PM_FREQB, PM_VFOA], ft1000()),
        (
            "6 FREQ+FREQB",
            vec![PM_FREQ, PM_FREQB],
            vec![format!("Freq={rx}"), format!("FreqB={tx}")],
        ),
        (
            "1 beats 2",
            vec![PM_FREQA, PM_FREQB, PM_VFOAB, PM_FREQ, PM_VFOEQUAL],
            ts570(),
        ),
        (
            "2 beats 3, 4",
            vec![PM_FREQ, PM_VFOEQUAL, PM_VFOA, PM_VFOB, PM_VFOSWAP],
            ic746(),
        ),
        (
            "3 beats 4, 5",
            vec![PM_VFOB, PM_FREQ, PM_VFOA, PM_VFOSWAP, PM_FREQA, PM_FREQB],
            ft100(),
        ),
        (
            "4 beats 5",
            vec![PM_FREQ, PM_VFOSWAP, PM_FREQA, PM_FREQB, PM_VFOA],
            ft817(),
        ),
        (
            "5 beats 6",
            vec![PM_FREQA, PM_FREQB, PM_VFOA, PM_FREQ],
            ft1000(),
        ),
        (
            "6 needs FREQA not writeable",
            vec![PM_FREQ, PM_FREQA, PM_FREQB],
            vec![],
        ),
    ];
    for (name, params, want) in cases {
        let (fake, mut rig) = online_rig(&params);
        assert!(rig.set_split_mode(144_300_312, F, RigMode::None), "{name}");
        assert_eq!(fake.log(), want, "{name}");
    }
}

/// The mode writes inside each split branch (OmniRigX.cs:1135, 1145, 1158/1165,
/// 1176/1183, 1195) and the VFOEQUAL pre-block (:1119-1126).
#[test]
fn split_branch_mode_writes() {
    let (rx, tx) = ("144300312", "144300000");
    let m = "Mode=PM_SSB_U";
    let cases: Vec<(&str, Vec<i32>, Vec<String>)> = vec![
        (
            "1 TS-570",
            vec![PM_FREQA, PM_FREQB, PM_VFOAB],
            vec![
                "Vfo=PM_VFOAB".into(),
                format!("FreqA={rx}"),
                format!("FreqB={tx}"),
                m.into(),
            ],
        ),
        (
            "3 FT-100D",
            vec![PM_VFOB, PM_FREQ, PM_VFOA],
            vec![
                "Vfo=PM_VFOB".into(),
                format!("Freq={tx}"),
                m.into(),
                "Vfo=PM_VFOA".into(),
                format!("Freq={rx}"),
                m.into(),
            ],
        ),
        (
            "4 FT-817",
            vec![PM_FREQ, PM_VFOSWAP],
            vec![
                "Vfo=PM_VFOSWAP".into(),
                format!("Freq={tx}"),
                m.into(),
                "Vfo=PM_VFOSWAP".into(),
                format!("Freq={rx}"),
                m.into(),
            ],
        ),
        (
            "5 FT-1000MP",
            vec![PM_FREQA, PM_FREQB, PM_VFOA],
            vec![
                "Vfo=PM_VFOA".into(),
                format!("FreqA={rx}"),
                format!("FreqB={tx}"),
                m.into(),
            ],
        ),
    ];
    for (name, mut params, want) in cases {
        params.extend(ALL_MODES);
        let (fake, mut rig) = online_rig(&params);
        assert!(rig.set_split_mode(144_300_312, F, RigMode::Usb), "{name}");
        assert_eq!(fake.log(), want, "{name}");
    }
}

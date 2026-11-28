//! The map draws the upstream plane icon (`AirScout/plane.png`), recoloured and rotated as
//! `CreatePlaneIcon` and `RotateImageByAngle` do, instead of a plain triangle.

use airscout::ui::map::{plane_icon_image, plane_icon_mesh};
use eframe::egui::{Color32, Pos2, TextureId};

#[test]
fn plane_icon_is_white_with_the_upstream_alpha() {
    let img = plane_icon_image();
    assert_eq!(img.size, [128, 128]);
    let px = |x: usize, y: usize| img.pixels[y * 128 + x];
    // transparent corner, opaque fuselage (nose up: column 62, rows 16..100)
    assert_eq!(px(0, 0).a(), 0);
    assert_eq!(px(127, 127).a(), 0);
    for y in [20, 60, 90] {
        assert_eq!(px(63, y), Color32::WHITE, "row {y}");
    }
    // the wings span the width around row 80, the nose is narrow at row 20
    assert!(px(20, 80).a() > 128);
    assert_eq!(px(20, 20).a(), 0);
}

#[test]
fn plane_icon_mesh_rotates_along_the_track_and_tints() {
    let c = Pos2::new(100.0, 100.0);
    let fill = Color32::from_rgb(0, 200, 0);
    let close = |a: Pos2, b: Pos2| (a - b).length() < 1e-3;

    // track 0: north up, the top-left texture corner is top-left on screen
    let m = plane_icon_mesh(TextureId::default(), c, 20.0, 0.0, fill);
    assert_eq!(m.indices, [0, 1, 2, 0, 2, 3]);
    assert!(close(m.vertices[0].pos, Pos2::new(90.0, 90.0)));
    assert_eq!(m.vertices[0].uv, Pos2::new(0.0, 0.0));
    assert!(m.vertices.iter().all(|v| v.color == fill));

    // track 90: heading east, the top-left texture corner moves to the top right
    let m = plane_icon_mesh(TextureId::default(), c, 20.0, 90.0, fill);
    assert!(close(m.vertices[0].pos, Pos2::new(110.0, 90.0)));
    // the nose (top edge centre) points east
    let nose = Pos2::new(
        (m.vertices[0].pos.x + m.vertices[1].pos.x) / 2.0,
        (m.vertices[0].pos.y + m.vertices[1].pos.y) / 2.0,
    );
    assert!(close(nose, Pos2::new(110.0, 100.0)));

    // a track outside 0..=360 draws north up, as the C# guard does
    for bad in [-1.0, 361.0, f64::NAN] {
        let m = plane_icon_mesh(TextureId::default(), c, 20.0, bad, fill);
        assert!(close(m.vertices[0].pos, Pos2::new(90.0, 90.0)), "{bad}");
    }
}

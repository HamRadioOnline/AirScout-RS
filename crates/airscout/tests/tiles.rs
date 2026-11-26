//! Tile provider list and HTTP options of the map.
use airscout::tiles::{
    http_options, provider_by_name, providers, tile_url, tiles_cache_dir, ProviderSource,
};
use as_settings::Secret;
use std::path::Path;
use walkers::sources::TileUrls;
use walkers::TileId;

#[test]
fn osm_provider_url_and_attribution() {
    let osm = &providers(&Secret::default())[0];
    assert_eq!(osm.name, "OpenStreetMap");
    assert_eq!(osm.url, "https://tile.openstreetmap.org/{z}/{x}/{y}.png");
    assert_eq!(osm.attribution, "© OpenStreetMap contributors");
}

#[test]
fn provider_list_and_lookup() {
    let none = Secret::default();
    let names: Vec<&str> = providers(&none).iter().map(|p| p.name).collect();
    assert_eq!(names, ["OpenStreetMap", "OpenTopoMap"]);
    assert_eq!(provider_by_name("OpenTopoMap", &none).name, "OpenTopoMap");
    // Thunderforest without a key, or an unknown name, falls back to OSM
    assert_eq!(
        provider_by_name("OCM Landscape", &none).name,
        "OpenStreetMap"
    );
    assert_eq!(provider_by_name("nope", &none).name, "OpenStreetMap");
}

#[test]
fn http_options_use_honest_user_agent() {
    let o = http_options(Path::new("/tmp/tiles"));
    let ua = o.user_agent.expect("user agent set");
    let ua = ua.to_str().expect("ascii");
    assert_eq!(ua, as_net::user_agent());
    assert!(!ua.contains("Mozilla"));
    assert_eq!(o.cache.as_deref(), Some(Path::new("/tmp/tiles")));
}

#[test]
fn cache_dir_is_below_data_dir() {
    assert_eq!(
        tiles_cache_dir(Path::new("/data")),
        Path::new("/data").join("tiles")
    );
}

#[test]
fn thunderforest_only_with_key() {
    let key = Secret::from("k3y-secret");
    let names: Vec<&str> = providers(&key).iter().map(|p| p.name).collect();
    assert_eq!(names, ["OpenStreetMap", "OpenTopoMap", "OCM Landscape"]);
    let tf = provider_by_name("OCM Landscape", &key);
    assert_eq!(
        tile_url(&tf, 1, 2, 3),
        "https://tile.thunderforest.com/landscape/3/1/2.png?apikey=k3y-secret"
    );
    assert!(tf.attribution.contains("Thunderforest"));
}

#[test]
fn tile_url_with_key_not_in_status() {
    let key = Secret::from("k3y-secret");
    let tf = provider_by_name("OCM Landscape", &key);
    // the key is only in the built tile URL, never in the template, Debug or attribution
    assert!(tf.url.contains("{apikey}"));
    assert!(!tf.url.contains("k3y-secret"));
    assert!(!format!("{tf:?}").contains("k3y-secret"));
    let src = ProviderSource(tf.clone());
    let id = TileId {
        x: 1,
        y: 2,
        zoom: 3,
    };
    assert!(src.tile_url(id).contains("k3y-secret"));
    let a = src.attribution();
    assert!(!a.text.contains("k3y-secret") && !a.url.contains("k3y-secret"));
}

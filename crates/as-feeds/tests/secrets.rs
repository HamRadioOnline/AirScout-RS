//! Every credential-like setting of every feed is declared in `secret_keys()`.
use as_feeds::registry::all_feeds;

/// Keys that match the credential name pattern but hold no credential.
const NOT_SECRET: &[(&str, &str)] = &[
    // OpenSky: URL of the token endpoint, not a token.
    ("OpenSky", "token_url"),
];

#[test]
fn credential_feeds_declare_secret_keys() {
    let pats = ["user", "pass", "token", "secret", "key", "auth"];
    for feed in all_feeds() {
        let secret = feed.secret_keys();
        for key in feed.settings().keys() {
            let lower = key.to_lowercase();
            if !pats.iter().any(|p| lower.contains(p)) {
                continue;
            }
            if NOT_SECRET.contains(&(feed.name(), key.as_str())) {
                continue;
            }
            assert!(
                secret.contains(&key.as_str()),
                "{}: key `{key}` looks like a credential but is not in secret_keys()",
                feed.name()
            );
        }
        for k in secret {
            assert!(
                feed.settings().contains_key(*k),
                "{}: secret key `{k}` is not a setting",
                feed.name()
            );
        }
    }
}

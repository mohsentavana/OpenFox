/// Lists ship inside the binary: no download on first run, no phone-home for
/// updates. The bundle is hosts-style rules over the ad and tracker domains
/// that dominate real-world filter lists; the import path (`add_list`)
/// accepts full easylist syntax for users who bring their own.
///
/// Remote sources are opt-in and fetched by the caller (the app, through
/// `openfox-net`) — this crate stays free of IO.
pub const DEFAULT_SOURCES: &[(&str, &str)] = &[
    ("easylist", "https://easylist.to/easylist/easylist.txt"),
    ("easyprivacy", "https://easylist.to/easylist/easyprivacy.txt"),
];

const BUILTIN_ADS: &str = include_str!("lists/ads.txt");
const BUILTIN_TRACKERS: &str = include_str!("lists/trackers.txt");

pub fn builtin_ads() -> &'static str {
    BUILTIN_ADS
}

pub fn builtin_trackers() -> &'static str {
    BUILTIN_TRACKERS
}

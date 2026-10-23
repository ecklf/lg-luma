//! Curated artwork is an exact application-ID allowlist. Inventory titles and
//! partial ID matches must never replace another application's installed icon.
#[derive(Debug)]
pub struct Brand {
    pub key: &'static str,
    pub title: &'static str,
    pub ids: &'static [&'static str],
    pub category: &'static str,
}

impl Brand {
    /// Optical bounds, rather than a shared bounding box: heavy wordmarks need
    /// more breathing room than wide, fine-lettered logos such as YouTube.
    pub fn artwork_size(&self) -> (f32, f32) {
        match self.key {
            "netflix" => (0.76, 88.),
            "youtube" => (0.74, 88.),
            "apple-tv" => (0.43, 88.),
            "disney-plus" => (0.72, 105.6),
            "prime-video" => (0.72, 88.),
            "jellyfin" => (0.83, 88.),
            "arte" => (0.58, 88.),
            "zdf" => (0.50, 88.),
            "das-erste" => (0.70, 88.),
            "hulu" | "joyn" | "itvx" => (0.62, 48.),
            "plex" => (0.62, 52.),
            "filmin" => (0.62, 46.),
            "hbo-max" | "bbc-iplayer" | "crunchyroll" | "twitch" | "dazn" | "paramount-plus"
            | "channel-4" | "wavve" => (0.70, 64.),
            "now-tv" => (0.70, 52.),
            "live-tv" | "browser" => (0.62, 68.),
            _ => (0.70, 56.),
        }
    }
}

const fn video(key: &'static str, title: &'static str, ids: &'static [&'static str]) -> Brand {
    Brand {
        key,
        title,
        ids,
        category: "Streaming",
    }
}
const fn music(key: &'static str, title: &'static str, ids: &'static [&'static str]) -> Brand {
    Brand {
        key,
        title,
        ids,
        category: "Music",
    }
}

pub const BRANDS: &[Brand] = &[
    video("live-tv", "Live TV", &["com.webos.app.livetv"]),
    Brand {
        key: "browser",
        title: "Web Browser",
        ids: &["com.webos.app.browser"],
        category: "Utilities",
    },
    video("netflix", "Netflix", &["netflix"]),
    video("youtube", "YouTube", &["youtube.leanback.v4"]),
    video(
        "apple-tv",
        "Apple TV",
        &["com.apple.appletv", "com.apple.appletv.web"],
    ),
    video("disney-plus", "Disney+", &["com.disney.disneyplus-prod"]),
    video("prime-video", "Prime Video", &["amazon"]),
    video("jellyfin", "Jellyfin", &["org.jellyfin.webos"]),
    video("arte", "ARTE", &["com.arte.tv", "com.3827031.168353"]),
    video("zdf", "ZDF", &["de.zdf.app.zdfm3"]),
    video("das-erste", "ARD Mediathek", &["ard.mediathek"]),
    music(
        "spotify",
        "Spotify",
        &["spotify-beehive", "com.spotify.tv.webos"],
    ),
    video("plex", "Plex", &["cdp-30", "plex"]),
    video(
        "hbo-max",
        "HBO Max",
        &["com.wbd.stream", "com.hbo.hbomax", "hbo-go-2"],
    ),
    video(
        "paramount-plus",
        "Paramount+",
        &["com.pplus.webapp.prod-intl", "cbs"],
    ),
    video("peacock", "Peacock", &["com.peacocktv.peacock"]),
    video("hulu", "Hulu", &["hulu"]),
    video(
        "crunchyroll",
        "Crunchyroll",
        &["crunchyroll", "com.crunchyroll.webos"],
    ),
    video("twitch", "Twitch", &["tv.twitch.tv.starshot.lg", "twitch"]),
    video("dazn", "DAZN", &["dazn"]),
    video("rakuten-tv", "Rakuten TV", &["ui30"]),
    music(
        "apple-music",
        "Apple Music",
        &["com.apple.music", "com.apple.ampl"],
    ),
    music(
        "amazon-music",
        "Amazon Music",
        &["com.theadelab.amclient-lg"],
    ),
    video(
        "bbc-iplayer",
        "BBC iPlayer",
        &["bbc.iplayer.3.0", "bbc.iplayer"],
    ),
    video("itvx", "ITVX", &["com.fvp.itv", "itvx"]),
    video(
        "channel-4",
        "Channel 4",
        &["com.fvp.ch4", "channel4", "all4.app"],
    ),
    video("now-tv", "NOW / Now TV", &["now.tv", "nowtv"]),
    video("rtl-plus", "RTL+", &["tvnow", "com.tvnow.webos"]),
    video("joyn", "Joyn", &["joyn"]),
    video("waipu-tv", "waipu.tv", &["tv.waipu.app.waipu-lg"]),
    video("zattoo", "Zattoo", &["zattoo", "com.zattoo.webapp"]),
    video(
        "skyshowtime",
        "SkyShowtime",
        &["com.skyshowtime.tv", "com.skyshowtime.skyshowtime"],
    ),
    video("coupang-play", "Coupang Play", &["coupangplay"]),
    video("tving", "TVING", &["cj.eandm"]),
    video("wavve", "Wavve", &["pooq"]),
    video("watcha", "Watcha Play", &["com.frograms.watchaplay.webos"]),
    video("movistar-plus", "Movistar+", &["telefonica"]),
    video("filmin", "Filmin", &["filmin"]),
];

pub fn for_app(id: &str) -> Option<&'static Brand> {
    BRANDS.iter().find(|brand| brand.ids.contains(&id))
}

pub fn for_key(key: &str) -> Option<&'static Brand> {
    BRANDS.iter().find(|brand| brand.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_curated_id_is_unique_and_selects_its_brand_without_title_matching() {
        let mut ids = HashSet::new();
        for brand in BRANDS {
            assert!(!brand.ids.is_empty());
            assert!(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("assets/logos/{}.svg", brand.key))
                    .is_file(),
                "{} has no vector source",
                brand.key
            );
            for id in brand.ids {
                assert!(ids.insert(id), "Duplicate curated app ID: {id}");
                let app = crate::model::App::new(id, "A localized application title");
                assert_eq!(app.brand, brand.key);
                assert_eq!(app.category, brand.category);
                assert!(for_app(&format!("{id}.unrelated")).is_none());
            }
        }
    }

    #[test]
    fn now_tv_aliases_select_now_without_rebranding_other_sky_or_now_apps() {
        for id in ["now.tv", "nowtv"] {
            assert_eq!(for_app(id).unwrap().key, "now-tv");
        }
        for id in [
            "de.sky.skyonline",
            "com.skygo.app.de.q",
            "now.tv.news",
            "com.nowtv.hk",
        ] {
            assert!(for_app(id).is_none());
        }
    }

    #[test]
    fn desktop_demo_contains_every_bundled_logo_and_no_duplicate_apps() {
        let inventory = crate::model::demo_inventory();
        let ids: HashSet<_> = inventory.apps.iter().map(|app| &app.id).collect();
        assert_eq!(ids.len(), inventory.apps.len());
        for brand in BRANDS {
            assert!(
                inventory.apps.iter().any(|app| app.brand == brand.key),
                "The desktop demo is missing {}",
                brand.title
            );
        }
        for entry in
            std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/logos"))
                .unwrap()
        {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "svg") {
                assert!(
                    for_key(path.file_stem().unwrap().to_str().unwrap()).is_some(),
                    "Bundled logo has no demo card: {}",
                    path.display()
                );
            }
        }
    }
}

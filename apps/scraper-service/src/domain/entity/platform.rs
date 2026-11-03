//! Platform classification for download targets.

/// A media platform the downloader can extract links from.
///
/// Classifying a URL is pure business logic — it knows nothing about HTTP,
/// Redis, or any provider API — so it lives in the domain. Infrastructure and
/// application both depend on this module; it depends on nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Instagram,
    Facebook,
    TikTok,
    YouTube,
    Spotify,
    Twitter,
    Pinterest,
    Reddit,
    Mega,
    TeraBox,
    GoogleDrive,
    MediaFire,
    PixelDrain,
    Threads,
    DoodStream,
    KrakenFiles,
    Danbooru,
    SoundCloud,
    Dailymotion,
    Streamable,
    Videy,
    Bilibili,
    /// No signature matched — callers fall back to the universal scraper.
    Unknown,
}

/// Registrable domains that identify a platform, in precedence order.
///
/// Order is significant: the first entry with a matching host wins. These are
/// matched against the URL's **host** (not the raw URL text) — see
/// [`Platform::detect`].
const SIGNATURES: &[(&[&str], Platform)] = &[
    (&["instagram.com", "instagr.am"], Platform::Instagram),
    (
        &["facebook.com", "fb.com", "fb.watch", "fbcdn.net"],
        Platform::Facebook,
    ),
    (
        &["tiktok.com", "tiktokcdn.com", "tiktokv.com"],
        Platform::TikTok,
    ),
    (
        &["youtube.com", "youtu.be", "youtubekids.com"],
        Platform::YouTube,
    ),
    (&["spotify.com", "spotify.link"], Platform::Spotify),
    (&["twitter.com", "x.com", "t.co"], Platform::Twitter),
    (
        &["pinterest.com", "pinterest.co.uk", "pin.it"],
        Platform::Pinterest,
    ),
    (&["reddit.com", "redd.it"], Platform::Reddit),
    (&["mega.nz", "mega.io"], Platform::Mega),
    (
        &["terabox.com", "4funbox.com", "nfile.io"],
        Platform::TeraBox,
    ),
    (
        &["drive.google.com", "docs.google.com"],
        Platform::GoogleDrive,
    ),
    (&["mediafire.com"], Platform::MediaFire),
    (&["pixeldrain.com"], Platform::PixelDrain),
    (&["threads.net", "threads.com"], Platform::Threads),
    (
        &[
            "dood.to",
            "dood.li",
            "dood.pm",
            "dood.so",
            "doodstream.com",
            "doodstre.am",
        ],
        Platform::DoodStream,
    ),
    (&["krakenfiles.com"], Platform::KrakenFiles),
    (
        &[
            "danbooru.donmai.us",
            "safebooru.donmai.us",
            "rule34.paheal.net",
        ],
        Platform::Danbooru,
    ),
    (&["soundcloud.com"], Platform::SoundCloud),
    (&["dailymotion.com", "dai.ly"], Platform::Dailymotion),
    (&["streamable.com"], Platform::Streamable),
    (&["videy.co", "videy.net"], Platform::Videy),
    (&["bilibili.com", "b23.tv"], Platform::Bilibili),
];

impl Platform {
    /// Classify a URL by matching its host against the signature table.
    ///
    /// Returns [`Platform::Unknown`] when nothing matches.
    ///
    /// Matching runs against the parsed host with subdomain suffixes, never
    /// against the raw URL text: `https://terabox.com/s/1` contains the
    /// literal substring `x.com` (inside `terabox.com`), so a substring match
    /// misclassifies every TeraBox link as Twitter.
    pub fn detect(url: &str) -> Self {
        let Some(host) = host_of(url) else {
            return Platform::Unknown;
        };
        SIGNATURES
            .iter()
            .find(|(domains, _)| domains.iter().any(|d| host_matches(&host, d)))
            .map_or(Platform::Unknown, |(_, platform)| *platform)
    }

    /// Stable lowercase identifier, used in API responses and cache keys.
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Instagram => "instagram",
            Platform::Facebook => "facebook",
            Platform::TikTok => "tiktok",
            Platform::YouTube => "youtube",
            Platform::Spotify => "spotify",
            Platform::Twitter => "twitter",
            Platform::Pinterest => "pinterest",
            Platform::Reddit => "reddit",
            Platform::Mega => "mega",
            Platform::TeraBox => "terabox",
            Platform::GoogleDrive => "gdrive",
            Platform::MediaFire => "mediafire",
            Platform::PixelDrain => "pixeldrain",
            Platform::Threads => "threads",
            Platform::DoodStream => "doodstream",
            Platform::KrakenFiles => "krakenfiles",
            Platform::Danbooru => "danbooru",
            Platform::SoundCloud => "soundcloud",
            Platform::Dailymotion => "dailymotion",
            Platform::Streamable => "streamable",
            Platform::Videy => "videy",
            Platform::Bilibili => "bilibili",
            Platform::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Extract the lowercase host from a URL, tolerating a missing scheme and
/// stripping userinfo, port, and a leading `www.`.
fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host_and_path = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let host = host_and_path.rsplit('@').next().unwrap_or(host_and_path);
    let host = host.split(':').next().unwrap_or(host);
    let host = host
        .strip_prefix("www.")
        .unwrap_or(host)
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(host)
}

/// True when `host` is `domain` itself or a subdomain of it.
fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_platform_from_url() {
        let cases = [
            ("https://www.instagram.com/p/A/", Platform::Instagram),
            ("https://fb.watch/xyz/", Platform::Facebook),
            ("https://vm.tiktok.com/ZM/", Platform::TikTok),
            ("https://youtu.be/dQw4w9WgXcQ", Platform::YouTube),
            ("https://open.spotify.com/track/1", Platform::Spotify),
            ("https://x.com/user/status/1", Platform::Twitter),
            ("https://t.co/abc", Platform::Twitter),
            ("https://www.reddit.com/r/rust/", Platform::Reddit),
            ("https://mega.nz/file/abc", Platform::Mega),
            ("https://drive.google.com/file/d/1", Platform::GoogleDrive),
            ("https://mediafire.com/file/x", Platform::MediaFire),
            ("https://b23.tv/abc", Platform::Bilibili),
            ("https://danbooru.donmai.us/posts/1", Platform::Danbooru),
            ("https://dood.to/e/abc", Platform::DoodStream),
            ("https://threads.net/@u/post/1", Platform::Threads),
        ];
        for (url, expected) in cases {
            assert_eq!(Platform::detect(url), expected, "url: {url}");
        }
    }

    #[test]
    fn host_containing_another_platform_fragment_is_not_a_match() {
        // "terabox.com" contains the literal substring "x.com", so a naive
        // substring match routes every TeraBox link to the Twitter downloader.
        assert_eq!(
            Platform::detect("https://terabox.com/s/1"),
            Platform::TeraBox
        );
        assert_eq!(
            Platform::detect("https://www.terabox.com/s/1"),
            Platform::TeraBox
        );
        assert_eq!(
            Platform::detect("https://x.com/user/status/1"),
            Platform::Twitter
        );
    }

    #[test]
    fn tolerates_missing_scheme_and_ports() {
        assert_eq!(Platform::detect("youtu.be/abc"), Platform::YouTube);
        assert_eq!(Platform::detect("https://x.com:443/u"), Platform::Twitter);
    }

    #[test]
    fn unknown_url_falls_back_to_unknown() {
        assert_eq!(Platform::detect("https://example.com/x"), Platform::Unknown);
        assert_eq!(Platform::detect(""), Platform::Unknown);
        assert_eq!(Platform::Unknown.as_str(), "unknown");
    }

    #[test]
    fn every_platform_has_a_lowercase_identifier() {
        for (_, platform) in SIGNATURES {
            let id = platform.as_str();
            assert!(!id.is_empty());
            assert_eq!(id.to_ascii_lowercase(), id);
        }
    }
}

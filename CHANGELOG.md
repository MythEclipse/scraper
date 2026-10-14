## [1.0.1](https://github.com/asepharyana/scraper/compare/v1.0.0...v1.0.1) (2026-10-14)

# 1.0.0 (2026-10-14)


### Bug Fixes

* **anime:** parse otakudesu search results from .chivsrc li ([6bfffa5](https://github.com/asepharyana/scraper/commit/6bfffa5364a41b75a6f372ea2ed8e14414038a2d))
* **anime:** repair genre & latest endpoints for the new otakudesu layout ([e6d75ed](https://github.com/asepharyana/scraper/commit/e6d75edf3384f518acd444a34d3da5d90a497c78))
* **cache:** self-heal Redis connection + don't 500 on cache write failure ([7a12f0e](https://github.com/asepharyana/scraper/commit/7a12f0e9c8f377c49dfbf391926685d0ab196280))
* **config:** remove list_separator, breaks string fields ([62aa5b0](https://github.com/asepharyana/scraper/commit/62aa5b0e52859afe3ba9de1c7b11cfe2dacf6c2c))
* disable extra_float_digits startup param for PG16 compatibility ([8910643](https://github.com/asepharyana/scraper/commit/891064354b49ad91fedace7ca65c7964481715fe))
* **downloader:** real FB media for all URL shapes + public terabox proxy ([8b9bdeb](https://github.com/asepharyana/scraper/commit/8b9bdeb56a06ca759bd4ccc2b5404bdbbf4c5f41))
* **downloader:** terabox proxy as primary link + accept threads.net URLs ([7d68ff5](https://github.com/asepharyana/scraper/commit/7d68ff56201661371eb2e8abeb0b11c331c95d2f))
* integrate yt-dlp for Bilibili/SoundCloud/TikTok fallbacks; fix Bilibili BV ID regex; fix stdout pipe buffer truncation (Stdio::piped); fix duplicate comment lines ([dd49053](https://github.com/asepharyana/scraper/commit/dd490535fc14a7872de481e3a0c074bf2d364e49))
* **komik:** keep api.komiku.org for lists, source genre-list from komiku.org root ([80af7b9](https://github.com/asepharyana/scraper/commit/80af7b94608e883716a3839a5aa2292924517ad7)), closes [#Genre](https://github.com/asepharyana/scraper/issues/Genre)
* **komik:** use komiku.org instead of dead api.komiku.org root ([a46adf1](https://github.com/asepharyana/scraper/commit/a46adf1ce938c5f1d8eb34f529b162fd66f1f11b))
* make scrape_media.py path resolution robust + include it in Nix build ([4684a6a](https://github.com/asepharyana/scraper/commit/4684a6a75cd40c0548991534add532bf1fd12077))
* map ScrapingError to 502 Bad Gateway instead of 500 Internal Server Error; fix downr.org Cloudflare block; fix YouTube savetube DNS failure; fix twitter/pinterest error handling ([c3e6537](https://github.com/asepharyana/scraper/commit/c3e6537a6352607ae6912288b17d704b1103feea))
* only connect OTLP metrics when OTEL_EXPORTER_OTLP_ENDPOINT is set ([562ada6](https://github.com/asepharyana/scraper/commit/562ada69ec9e1430aae14829998049fa23c0c40e))
* replace dead YouTube APIs with yt-dlp subprocess; fix all JSON panic crashes; add yt-dlp fallback for TikTok/Bilibili/SoundCloud; fix Bilibili BV ID regex; fix stdout pipe buffer truncation (Stdio::piped) ([fb33b67](https://github.com/asepharyana/scraper/commit/fb33b67a2981d7f83119a1f321af5d4c47c2799f))
* replace dead YouTube APIs with yt-dlp subprocess; fix all JSON panic crashes; add yt-dlp helper functions ([a41425a](https://github.com/asepharyana/scraper/commit/a41425a8878dea18cf1c77736ffc299319d06ac8))
* revert to alqanime.si, use fetch_with_proxy for direct+relay ([052c6cf](https://github.com/asepharyana/scraper/commit/052c6cfeb8a21666827cfe31af5d879c6202420c))
* TikTok via embed-page scraping returns real MP4 (tiktok-embed provider) ([134fde4](https://github.com/asepharyana/scraper/commit/134fde4ac23f3b89decbb03b20e88fec4e3e28d8))
* Twitter/X via Syndication API returns real MP4s (twitter-syndication provider) ([563bce5](https://github.com/asepharyana/scraper/commit/563bce5211637a9c059fc47d30e75ec920b56db7))
* update notify-parent target to asepharyana/asepharyana-hub ([1ddcd15](https://github.com/asepharyana/scraper/commit/1ddcd155403422c0c75968a11b2b62f218dd43e9))
* **youtube:** filter broken formats, use bestvideo+bestaudio for merged output ([5cec284](https://github.com/asepharyana/scraper/commit/5cec284cd654bdff108316effb401e3c8c5e9716))
* **youtube:** filter m3u8/hls protocol streams + check requested_formats ([8a55fd9](https://github.com/asepharyana/scraper/commit/8a55fd91a4fe117286041375f773ab8144ea49de))


### Features

* add Dailymotion + Reddit downloader endpoints via yt-dlp ([7850b5f](https://github.com/asepharyana/scraper/commit/7850b5fa03b2e94eef3c7a9d7e71b0799d8d2091))
* add dependabot auto-merge workflow ([c709c84](https://github.com/asepharyana/scraper/commit/c709c84a4d423381bfbd7eda317c121786f8dc5d))
* add dependabot config (npm auto-deps) ([a23698a](https://github.com/asepharyana/scraper/commit/a23698a6c4efea48752b4f20ed75f7eaa2ff93d9))
* add episode list with download URLs to detail endpoint ([cd4e3ae](https://github.com/asepharyana/scraper/commit/cd4e3aee20a48a624a2639cc601fe8d122e7af80))
* add Playwright browser scraping fallback for Instagram/Facebook/Twitter ([4c5dfdb](https://github.com/asepharyana/scraper/commit/4c5dfdb1568f10af403ba08a28fc7ab434c574a5))
* add Streamable downloader endpoint + fix detect false-positive ([dfa37f8](https://github.com/asepharyana/scraper/commit/dfa37f86e3e8a94c76facdba2a9ed36df81e32f4))
* **ai:** add pollinations chat, gemini, image endpoints ([e6f5430](https://github.com/asepharyana/scraper/commit/e6f543078b92468a8e8259d6d6e616568dd891d8))
* **downloader:** add 26 downloader APIs + detect endpoint ([763a641](https://github.com/asepharyana/scraper/commit/763a641593ff3f3cd1f9ee2c740bdab3a26a69cb))
* **downloader:** route terabox via local Playwright resolver ([adfbbfc](https://github.com/asepharyana/scraper/commit/adfbbfcea7d531a1bdccf2eb5c192308bb17e4b1))
* **downloader:** youtube progressive-first + server-side merge ([029837d](https://github.com/asepharyana/scraper/commit/029837d0f19e2046775f69fa2bb71671bc732e9f))
* full clean architecture refactor — domain → application → infrastructure → presentation ([776fa82](https://github.com/asepharyana/scraper/commit/776fa828d8e961101f148b140bb348302fe31f40))
* **image:** add brat + animated brat image generation ([4efcf1f](https://github.com/asepharyana/scraper/commit/4efcf1f0eaeb35aa9ae35c3838da744ff5718a7b))
* migrate anime2 scraper from alqanime.si to alqanime.net ([c035601](https://github.com/asepharyana/scraper/commit/c035601a366691e3841c11a49f541bae3cb0289f))
* **misc:** add misc endpoints (currency-converter, harga-emas, kurs-bca, server-info) ([6df8af5](https://github.com/asepharyana/scraper/commit/6df8af54eb6ade25dae66d919d3694dc01e34d59))
* Pinterest Playwright fallback + Spotify graceful DRM message ([6b3fad1](https://github.com/asepharyana/scraper/commit/6b3fad1ad443e3b03dafa1ff5ee57fd84e5de1ad))
* remove ai, uploader, and weebs modules ([5470d74](https://github.com/asepharyana/scraper/commit/5470d74921512ed1878b4372389e5b0f5d08e8a5))
* remove croxy proxy and image cache features ([5c51ec8](https://github.com/asepharyana/scraper/commit/5c51ec895f5f697f6d32a23fc282fd4564dac5eb))
* **search:** add bmkg, jadwal-sholat, weather, yt-search, google endpoints ([7ffc1bd](https://github.com/asepharyana/scraper/commit/7ffc1bdd812722fbcd994057d19c2ecac2e62549))
* Spotify via oEmbed+ytsearch, Pinterest via pinterest-dl CLI scraping ([4dee99c](https://github.com/asepharyana/scraper/commit/4dee99cc43c7cfb2811a3bcf258f8ec0a195a3d2))
* **stalk:** add github + youtube profile stalk endpoints ([242e349](https://github.com/asepharyana/scraper/commit/242e3498fe8c82fec8971312210565f8cabb1b4a))
* **stalk:** add mobile-legends, free-fire, genshin, twitter, tiktok, instagram ([8d74d04](https://github.com/asepharyana/scraper/commit/8d74d04b4372bd33bc7c5dd3f14a2d772de408da))
* **stalk:** remove dead mobile-legends, free-fire, tiktok, instagram ([93828c7](https://github.com/asepharyana/scraper/commit/93828c7253951065e9360d053c5fd1450782ba77))
* **stalk:** remove fake genshin stalk ([09c706c](https://github.com/asepharyana/scraper/commit/09c706cc61f447a9a119e35a3be5f2ad7c5c3bb1))
* **tools:** add whois(RDAP), iplocation, tinyurl, check-hosting, cek-resi, hargapangan ([278d4fe](https://github.com/asepharyana/scraper/commit/278d4fe32a6fb56516425e07380beebb85ca8ea6))
* update dependabot config ([55cfd51](https://github.com/asepharyana/scraper/commit/55cfd513e3f848b6b0dc3df9b869edf6f124dc8e))
* update dependabot config ([380ecfc](https://github.com/asepharyana/scraper/commit/380ecfcd9bbc75b41c4aaeab68aff0fb0497701c))
* **uploader:** add ryzencdn local-disk uploader (POST multipart + GET serve) ([2769b11](https://github.com/asepharyana/scraper/commit/2769b11918d69802263e007a7b822ecb6f55fc7a))
* **weebs:** add anime/manga info, whatanime, waifu endpoints ([091a68b](https://github.com/asepharyana/scraper/commit/091a68bd555fdb1728684be3b13e0a960698a6e5))
* wire videy + tiktok/v2 + twitter/v2 downloader endpoints ([dc44085](https://github.com/asepharyana/scraper/commit/dc44085cad62041e4327495bf134af0ef7b9c21d))


### Performance Improvements

* **downloader:** compile regex/selector literals once via LazyLock ([28eeaf0](https://github.com/asepharyana/scraper/commit/28eeaf098e271d19c4ef080558e72b17c2ca8630))

# Changelog

All notable changes to scraper are documented here.
Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
and are derived automatically from
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

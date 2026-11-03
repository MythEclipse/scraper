//! The wire contract: DTOs must serialize to exactly the JSON the domain
//! entities produced before the mirror was introduced.
//!
//! The fixtures in `tests/fixtures/wire/` were recorded from the domain
//! entities themselves, so this test fails if a conversion drops, renames, or
//! reorders a field — the kind of drift that no compiler check can catch.

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use scraper_service::presentation::dto::entities::alqanime::AlqDetailDataDto;
use scraper_service::presentation::dto::entities::anime::{
    Anime2ItemDetailDto, AnimeDetailDataDto, AnimeFullDataDto, PaginationDto,
};
use scraper_service::presentation::dto::entities::downloader::DownloadResultDto;
use scraper_service::presentation::dto::entities::komik::DetailDataDto;

mod support;

fn assert_wire_shape<T: Serialize>(name: &str, dto: T) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/wire")
        .join(format!("{name}.json"));
    let golden = fs::read_to_string(&path).expect("golden fixture is readable");
    let expected: Value = serde_json::from_str(&golden).expect("golden is valid JSON");
    let actual = serde_json::to_value(&dto).expect("dto serializes");

    assert_eq!(actual, expected, "wire contract drifted for {name}");
}

#[test]
fn pagination_matches() {
    assert_wire_shape(
        "pagination",
        PaginationDto::from(support::wire_fixtures::pagination()),
    );
}

#[test]
fn anime_detail_matches() {
    assert_wire_shape(
        "anime_detail",
        AnimeDetailDataDto::from(support::wire_fixtures::anime_detail()),
    );
}

#[test]
fn anime_full_matches() {
    assert_wire_shape(
        "anime_full",
        AnimeFullDataDto::from(support::wire_fixtures::anime_full()),
    );
}

#[test]
fn anime2_detail_matches() {
    assert_wire_shape(
        "anime2_detail",
        Anime2ItemDetailDto::from(support::wire_fixtures::anime2_detail()),
    );
}

#[test]
fn komik_detail_matches() {
    assert_wire_shape(
        "komik_detail",
        DetailDataDto::from(support::wire_fixtures::komik_detail()),
    );
}

#[test]
fn alq_detail_matches() {
    assert_wire_shape(
        "alq_detail",
        AlqDetailDataDto::from(support::wire_fixtures::alq_detail()),
    );
}

#[test]
fn download_result_matches() {
    assert_wire_shape(
        "download_result",
        DownloadResultDto::from(support::wire_fixtures::download_result()),
    );
}

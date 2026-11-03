//! Request schemas: the constraints `serde` cannot express.
//!
//! `validate` is the single entry point handlers use, so these cases pin the
//! contract: a required value must be non-empty, an optional value may be
//! absent, and a violation becomes a 400 (`AppError::BadRequest`).

use scraper_service::presentation::handler::image::BratParams;
use scraper_service::presentation::handler::stalk::StalkParams;
use scraper_service::presentation::handler::tools::{DomainParams, IpParams};
use scraper_service::presentation::validation::validate;

#[test]
fn required_value_must_not_be_empty() {
    let error = validate(DomainParams {
        domain: Some(String::new()),
    })
    .expect_err("empty domain must be rejected");

    assert!(
        error.to_string().contains("Invalid request"),
        "unexpected error: {error}"
    );
}

#[test]
fn required_value_is_accepted() {
    let params = validate(DomainParams {
        domain: Some("example.com".to_string()),
    })
    .expect("valid domain passes");
    assert_eq!(params.domain.as_deref(), Some("example.com"));
}

#[test]
fn missing_optional_value_is_allowed() {
    let params = validate(StalkParams {
        username: None,
        user_id: None,
        userid: None,
        zone_id: None,
        zoneid: None,
    })
    .expect("all-absent params pass: the handler decides what it needs");
    assert!(params.username.is_none());
}

#[test]
fn overlong_value_is_rejected() {
    let error = validate(IpParams {
        ip: Some("1".repeat(64)),
    })
    .expect_err("64-char IP must be rejected");

    assert!(error.to_string().contains("Invalid request"));
}

#[test]
fn text_length_ceiling_is_enforced() {
    let error = validate(BratParams {
        text: Some("x".repeat(501)),
    })
    .expect_err("501-char brat text must be rejected");

    assert!(error.to_string().contains("Invalid request"));
}

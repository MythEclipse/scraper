//! Request validation at the presentation boundary.
//!
//! Each request DTO carries its own [`zod_rs`] schema (`#[derive(ZodSchema)]`
//! plus `#[zod(..)]` field constraints). The derive generates
//! `validate_and_parse`, and [`ValidatedRequest`] names that capability so
//! handlers can call one uniform [`validate`].

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::presentation::error::AppError;

/// A request DTO that can be checked against its own schema.
pub trait ValidatedRequest: Sized + Serialize + DeserializeOwned {
    fn validate_against_schema(value: &Value) -> Result<Self, zod_rs::__private::ValidationResult>;
}

/// Check a deserialized request against its schema, mapping failures to 400.
///
/// The DTO is already typed; the schema re-checks the *values* (lengths,
/// formats, ranges) that `serde` cannot express.
pub fn validate<T: ValidatedRequest>(request: T) -> Result<T, AppError> {
    let value = serde_json::to_value(&request)
        .map_err(|error| AppError::Internal(format!("request re-encode failed: {error}")))?;
    T::validate_against_schema(&value).map_err(|issues| {
        tracing::warn!(%issues, "request validation failed");
        AppError::BadRequest(format!("Invalid request:{}", issues))
    })
}

/// Wire a `#[derive(ZodSchema)]` request DTO into [`ValidatedRequest`].
macro_rules! impl_validated_request {
    ($($request:ty),* $(,)?) => {
        $(
            impl ValidatedRequest for $request {
                fn validate_against_schema(
                    value: &Value,
                ) -> Result<Self, zod_rs::__private::ValidationResult> {
                    Self::validate_and_parse(value)
                }
            }
        )*
    };
}

impl_validated_request!(
    crate::presentation::handler::downloader::DownloadParams,
    crate::presentation::handler::downloader::TeraboxProxyParams,
    crate::presentation::handler::image::BratParams,
    crate::presentation::handler::misc::CurrencyParams,
    crate::presentation::handler::search::SearchParams,
    crate::presentation::handler::stalk::StalkParams,
    crate::presentation::handler::tools::DomainParams,
    crate::presentation::handler::tools::IpParams,
    crate::presentation::handler::tools::UrlParams,
    crate::presentation::handler::tools::ResiParams,
    crate::presentation::handler::anime2::FilterQuery,
);

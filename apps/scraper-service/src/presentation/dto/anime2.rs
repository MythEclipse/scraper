//! Anime2 (Alqanime) response DTOs.
//!
//! These are the HTTP wrappers: the use cases return domain types and the
//! handler converts here, so the wire shape is owned by presentation.

use serde::Serialize;
use utoipa::ToSchema;

use crate::presentation::dto::entities::alqanime::AlqDetailDataDto;
use crate::presentation::dto::entities::anime::{
    Anime2DataDto, FilterAnimeItemDto, GenreDto, PaginationDto,
};

/// Index payload: ongoing + complete listings.
#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct Anime2Response {
    pub status: String,
    pub data: Anime2DataDto,
}

/// Genre list payload.
#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct GenresResponse {
    pub status: String,
    pub data: Vec<GenreDto>,
}

/// Echo of the filters the client asked for.
#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct FiltersApplied {
    pub genre: Option<String>,
    pub status: Option<String>,
    pub r#type: Option<String>,
    pub order: Option<String>,
}

/// Filter results payload.
#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct FilterResponse {
    pub success: bool,
    pub data: Vec<FilterAnimeItemDto>,
    pub pagination: PaginationDto,
    pub filters_applied: FiltersApplied,
    pub status: String,
}

/// Detail payload.
#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct DetailResponse {
    pub status: String,
    pub data: AlqDetailDataDto,
}

//! Komik API response DTOs.

use serde::Serialize;
use utoipa::ToSchema;

use crate::presentation::dto::entities::anime::PaginationDto;
use crate::presentation::dto::entities::komik::{
    ChapterDataDto, DetailDataDto, KomikGenreDto, KomikItemDto,
};

#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct GenresResponse {
    pub status: String,
    pub data: Vec<KomikGenreDto>,
}

#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct GenreKomikResponse {
    pub status: String,
    pub genre: String,
    pub data: Vec<KomikItemDto>,
    pub pagination: PaginationDto,
}

#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct DetailResponse {
    pub status: bool,
    pub data: DetailDataDto,
}

#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct ChapterResponse {
    pub message: String,
    pub data: ChapterDataDto,
}

#[derive(Serialize, Debug, Clone, ToSchema)]
pub struct SearchKomikResponse {
    pub status: String,
    pub data: Vec<KomikItemDto>,
    pub pagination: PaginationDto,
}

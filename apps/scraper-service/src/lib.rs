// Library root — clean architecture module structure

// ============================================================================
// Domain Layer — pure business logic, no framework dependencies
// ============================================================================
pub mod domain;

// ============================================================================
// Application Layer — use cases / business orchestration
// ============================================================================
pub mod application;

// ============================================================================
// Infrastructure Layer — implements domain ports
// ============================================================================
pub mod infrastructure;

// ============================================================================
// Presentation Layer — Axum handlers, DTOs, state, middleware
// ============================================================================
pub mod presentation;

// ============================================================================
// Core Framework & Infrastructure
// ============================================================================
pub mod bootstrap;
pub mod config;
pub mod events;
pub mod observability;

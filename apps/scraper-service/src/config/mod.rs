//! Type-safe application configuration.
//!
//! This module provides a strongly-typed configuration system that:
//! - Loads from environment variables and optional TOML files
//! - Fails fast at startup if required variables are missing
//! - Supports hierarchical configuration (default -> environment-specific)

use serde::Deserialize;
use serde_json::{Map, Value};
use std::env;
use std::path::Path;
use std::sync::LazyLock;

/// Configuration load failure.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot parse {path}: {source}")]
    Parse {
        path: String,
        source: toml::de::Error,
    },
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

/// Application configuration loaded at startup.
/// All fields are required unless marked as `Option<T>`.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// Database connection URL (PostgreSQL)
    pub database_url: String,

    /// Secret key for JWT signing
    pub jwt_secret: String,

    /// Redis connection URL
    #[serde(default)]
    pub redis_url: String,

    /// Server port to bind to
    #[serde(default = "default_port")]
    pub server_port: u16,

    /// Environment (development, staging, production)
    #[serde(default = "default_env")]
    pub environment: String,

    /// Allowed CORS origins (comma-separated in env var, parsed to Vec)
    #[serde(default, deserialize_with = "deserialize_comma_separated")]
    pub cors_origins: Vec<String>,

    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// SMTP configuration for emails (optional)
    pub smtp: Option<SmtpConfig>,

    /// Database pool configuration
    #[serde(default)]
    pub db: DbConfig,

    /// Domain/URL values that may change between deployments
    #[serde(default)]
    pub urls: UrlConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UrlConfig {
    #[serde(default = "default_site_url")]
    pub site_url: String,
}

impl Default for UrlConfig {
    fn default() -> Self {
        Self {
            site_url: default_site_url(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    #[serde(default = "default_db_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_db_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_db_connect_timeout")]
    pub connect_timeout_seconds: u64,
    #[serde(default = "default_db_idle_timeout")]
    pub idle_timeout_seconds: u64,
    #[serde(default = "default_db_acquire_timeout")]
    pub acquire_timeout_seconds: u64,
    #[serde(default = "default_db_max_lifetime")]
    pub max_lifetime_seconds: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            max_connections: default_db_max_connections(),
            min_connections: default_db_min_connections(),
            connect_timeout_seconds: default_db_connect_timeout(),
            idle_timeout_seconds: default_db_idle_timeout(),
            acquire_timeout_seconds: default_db_acquire_timeout(),
            max_lifetime_seconds: default_db_max_lifetime(),
        }
    }
}

/// SMTP configuration for sending emails
#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub from_name: String,
}

/// MinIO/S3-compatible storage configuration
#[derive(Debug, Clone)]
pub struct MinioConfig {
    /// MinIO endpoint URL (e.g., "https://cdn.asepharyana.my.id")
    pub endpoint: String,
    /// Bucket name
    pub bucket_name: String,
    /// Access key / username
    pub access_key: String,
    /// Secret key / password
    pub secret_key: String,
    /// Use HTTPS (true) or HTTP (false)
    pub secure: bool,
    /// AWS region (default: us-east-1)
    pub region: String,
    /// Public URL for serving files (optional)
    pub public_url: Option<String>,
    /// Prefix for avatar files (e.g., "avatars")
    pub avatar_prefix: String,
}

impl MinioConfig {
    /// Load MinIO configuration from environment variables
    pub fn from_env() -> Option<Self> {
        let endpoint = env::var("MINIO_ENDPOINT").ok()?;
        let bucket_name = env::var("MINIO_BUCKET_NAME").ok()?;
        let access_key = env::var("MINIO_ACCESS_KEY").ok()?;
        let secret_key = env::var("MINIO_SECRET_KEY").ok()?;

        let secure = env::var("MINIO_SECURE")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);

        let region = env::var("MINIO_REGION").unwrap_or_else(|_| "us-east-1".to_string());

        let public_url = env::var("MINIO_PUBLIC_URL").ok();

        let avatar_prefix =
            env::var("MINIO_AVATAR_PREFIX").unwrap_or_else(|_| "avatars".to_string());

        Some(Self {
            endpoint,
            bucket_name,
            access_key,
            secret_key,
            secure,
            region,
            public_url,
            avatar_prefix,
        })
    }
}

fn default_port() -> u16 {
    4091
}

fn default_env() -> String {
    "development".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_site_url() -> String {
    "https://asepharyana.my.id".to_string()
}

/// Deserialize a comma-separated string into a Vec<String>.
/// Used for fields like cors_origins that come from a single env var.
fn deserialize_comma_separated<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(s.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

fn default_db_max_connections() -> u32 {
    100
}

fn default_db_min_connections() -> u32 {
    10
}

fn default_db_connect_timeout() -> u64 {
    5
}

fn default_db_idle_timeout() -> u64 {
    300
}

fn default_db_acquire_timeout() -> u64 {
    10
}

fn default_db_max_lifetime() -> u64 {
    1800
}

/// Deep-merge an optional TOML config file into `into`.
///
/// A missing file is not an error: most deploys are env-only.
fn merge_file(into: &mut Map<String, Value>, path: &Path) -> Result<(), ConfigError> {
    if !path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let parsed: toml::Value = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.display().to_string(),
        source,
    })?;
    let value = serde_json::to_value(parsed).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    merge_value(into, value);
    Ok(())
}

/// Merge `value` into `into` at leaf level: nested objects merge recursively,
/// every other value replaces the previous one.
fn merge_value(into: &mut Map<String, Value>, value: Value) {
    let Value::Object(map) = value else {
        return;
    };
    for (key, incoming) in map {
        let both_objects = matches!(incoming, Value::Object(_))
            && matches!(into.get(&key), Some(Value::Object(_)));
        if both_objects {
            if let (Some(Value::Object(existing)), Value::Object(inner)) =
                (into.get_mut(&key), incoming)
            {
                merge_value(existing, Value::Object(inner));
            }
        } else {
            into.insert(key, incoming);
        }
    }
}

/// Merge every `{prefix}_*` env var into `into`, lowercased, with `__` opening
/// a nested level (`APP_DB__MAX_CONNECTIONS` → `db.max_connections`).
fn merge_env(into: &mut Map<String, Value>, prefix: &str) {
    let prefix = format!("{prefix}_");
    for (key, raw) in env::vars() {
        let Some(rest) = key.strip_prefix(&prefix) else {
            continue;
        };
        let path: Vec<String> = rest.split("__").map(str::to_ascii_lowercase).collect();
        set_path(into, &path, env_scalar(raw));
    }
}

/// Merge the legacy flat env vars, which win over `APP_*` and file values.
fn merge_legacy_env(into: &mut Map<String, Value>) {
    const LEGACY: [(&str, &str); 3] = [
        ("DATABASE_URL", "database_url"),
        ("JWT_SECRET", "jwt_secret"),
        ("REDIS_URL", "redis_url"),
    ];
    for (env_key, field) in LEGACY {
        if let Ok(raw) = env::var(env_key) {
            into.insert(field.to_string(), env_scalar(raw));
        }
    }
}

/// Read an env value as the JSON scalar it looks like, so `APP_SERVER_PORT=4091`
/// reaches `u16` fields. Anything that is not a number or bool stays a string
/// (`a.com,b.com` must not be parsed).
fn env_scalar(raw: String) -> Value {
    match serde_json::from_str::<Value>(&raw) {
        Ok(v @ (Value::Number(_) | Value::Bool(_))) => v,
        _ => Value::String(raw),
    }
}

/// Insert `value` at `path`, creating intermediate objects.
fn set_path(into: &mut Map<String, Value>, path: &[String], value: Value) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    if rest.is_empty() {
        into.insert(head.clone(), value);
        return;
    }
    let entry = into
        .entry(head.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Value::Object(child) = entry {
        set_path(child, rest, value);
    }
}

impl AppConfig {
    /// Load configuration from environment and optional config files.
    ///
    /// Priority (highest to lowest):
    /// 1. Legacy flat env vars (`DATABASE_URL`, `JWT_SECRET`, `REDIS_URL`)
    /// 2. Environment variables prefixed with `APP_` (`__` nests: `APP_DB__MAX_CONNECTIONS`)
    /// 3. `config/{run_mode}.toml`
    /// 4. `config/default.toml`
    pub fn load() -> Result<Self, ConfigError> {
        // Load .env file first
        if let Err(e) = dotenvy::dotenv() {
            tracing::debug!("Could not load .env file: {}", e);
        }

        let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        // Start with default config file (optional — many deploys are env-only)
        let mut merged = Map::new();
        merge_file(&mut merged, Path::new("config/default.toml"))?;
        // Layer on environment-specific values (optional)
        merge_file(&mut merged, Path::new(&format!("config/{run_mode}.toml")))?;
        merge_env(&mut merged, "APP");
        merge_legacy_env(&mut merged);

        serde_json::from_value(Value::Object(merged))
            .map_err(|e| ConfigError::Invalid(e.to_string()))
    }

    /// Check if running in production mode
    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }

    /// Check if running in development mode
    pub fn is_development(&self) -> bool {
        self.environment == "development"
    }
}

/// Global configuration instance, loaded once at startup.
/// Panics if configuration is invalid - this is intentional for fail-fast behavior.
pub static CONFIG: LazyLock<AppConfig> = LazyLock::new(|| {
    AppConfig::load().unwrap_or_else(|e| {
        eprintln!("❌ Failed to load configuration: {}", e);
        eprintln!("   Make sure all required environment variables are set:");
        eprintln!("   - DATABASE_URL");
        eprintln!("   - JWT_SECRET");
        eprintln!("   - REDIS_URL (or APP_REDIS_URL)");
        std::process::exit(1);
    })
});

/// Global MinIO configuration, loaded from environment variables.
/// Returns None if required MINIO_* variables are not set.
pub static MINIO_CONFIG: LazyLock<Option<MinioConfig>> = LazyLock::new(|| {
    let _ = dotenvy::dotenv();
    MinioConfig::from_env()
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_scalar_only_coerces_numbers_and_bools() {
        assert_eq!(env_scalar("4091".into()), Value::from(4091));
        assert_eq!(env_scalar("true".into()), Value::Bool(true));
        assert_eq!(
            env_scalar("a.com,b.com".into()),
            Value::String("a.com,b.com".into())
        );
        assert_eq!(
            env_scalar("https://x".into()),
            Value::String("https://x".into())
        );
    }

    #[test]
    fn set_path_nests_under_one_object() {
        let mut map = Map::new();
        set_path(
            &mut map,
            &["db".into(), "max_connections".into()],
            Value::from(50),
        );
        set_path(
            &mut map,
            &["db".into(), "min_connections".into()],
            Value::from(2),
        );
        assert_eq!(map["db"]["max_connections"], Value::from(50));
        assert_eq!(map["db"]["min_connections"], Value::from(2));
    }

    #[test]
    fn merge_value_is_leaf_level() {
        let mut merged = Map::new();
        merge_value(
            &mut merged,
            serde_json::json!({"db": {"max_connections": 50}, "log_level": "warn"}),
        );
        merge_value(
            &mut merged,
            serde_json::json!({"db": {"min_connections": 2}}),
        );
        assert_eq!(merged["db"]["max_connections"], Value::from(50));
        assert_eq!(merged["db"]["min_connections"], Value::from(2));
        assert_eq!(merged["log_level"], Value::String("warn".into()));
    }

    #[test]
    fn missing_config_file_is_not_an_error() {
        let mut map = Map::new();
        assert!(merge_file(&mut map, Path::new("config/does-not-exist.toml")).is_ok());
        assert!(map.is_empty());
    }
}

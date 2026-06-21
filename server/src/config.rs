use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    /// Base URL of the Audiobookshelf instance, no trailing slash.
    pub abs_base_url: String,
    /// TCP address to listen on (e.g. "0.0.0.0:3456"). Takes precedence over PORT.
    pub bind: String,
    /// Path to the compiled Vue SPA static files.
    pub static_dir: String,
    /// When true, skip ABS token validation (useful for integration tests).
    pub bypass_auth: bool,
    /// Allowed cross-origin for CORS (e.g. "http://localhost:5173" for Vite dev).
    /// Omit in production — the SPA is served from the same origin.
    pub cors_origin: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let bind = env::var("BIND").unwrap_or_else(|_| {
            env::var("PORT")
                .map(|p| format!("0.0.0.0:{p}"))
                .unwrap_or_else(|_| "0.0.0.0:3456".into())
        });
        Self {
            abs_base_url: env::var("ABS_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:13378".into()),
            bind,
            static_dir: env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()),
            bypass_auth: env::var("BYPASS_AUTH").map(|v| v == "1" || v == "true").unwrap_or(false),
            cors_origin: env::var("CORS_ORIGIN").ok(),
        }
    }

    /// Constructs a test-friendly config. Available in all build profiles so
    /// integration tests (which are separate crates) can call it.
    pub fn for_test() -> Self {
        Self {
            abs_base_url: "http://abs.test".into(),
            bind: "127.0.0.1:0".into(),
            static_dir: "./static".into(),
            bypass_auth: true,
            cors_origin: None,
        }
    }
}

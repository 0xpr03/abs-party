use anyhow::Result;
use reqwest::{Client, header};

use crate::models::{AbsLoginResponse, AbsMeResponse};

pub struct AbsClient {
    client: Client,
    base_url: String,
}

impl AbsClient {
    pub fn new(base_url: String) -> Self {
        let mut default_headers = header::HeaderMap::new();
        default_headers.insert(header::USER_AGENT, header::HeaderValue::from_static("abs-party/1.0"));
        default_headers.insert(header::ACCEPT, header::HeaderValue::from_static("application/json"));
        Self {
            client: Client::builder()
                .default_headers(default_headers)
                .build()
                .expect("failed to build reqwest client"),
            base_url,
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<AbsLoginResponse> {
        let resp = self
            .client
            .post(format!("{}/login", self.base_url))
            .json(&serde_json::json!({ "username": username, "password": password }))
            .send()
            .await?
            .error_for_status()?
            .json::<AbsLoginResponse>()
            .await?;
        Ok(resp)
    }

    pub async fn me(&self, token: &str) -> Result<AbsMeResponse> {
        let resp = self
            .client
            .get(format!("{}/api/me", self.base_url))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json::<AbsMeResponse>()
            .await?;
        Ok(resp)
    }

    /// `token: None` sends no Authorization header (for endpoints ABS serves without auth).
    pub async fn proxy_raw(
        &self,
        token: Option<&str>,
        method: reqwest::Method,
        url: &str,
        body: bytes::Bytes,
    ) -> Result<(u16, bytes::Bytes, Option<String>)> {
        let mut builder = self.client.request(method, url);
        match token {
            Some(t) => builder = builder.bearer_auth(t),
            // Unauthenticated requests are image fetches; the default Accept is JSON
            None => builder = builder.header(reqwest::header::ACCEPT, "image/*,*/*;q=0.8"),
        }
        if !body.is_empty() {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let resp = builder.send().await?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = resp.bytes().await?;
        Ok((status, bytes, content_type))
    }
}

//! HTTP adapter for the Person registry (R-DECL-4).
//!
//! Concrete implementation of `crate::application::PersonRegistryPort`
//! that asks the Person service `GET /v1/persons/{id}` and treats a
//! 2xx as "exists", a 404 as "does not exist", and any other status
//! as a transport error.
//!
//! Authentication: the adapter forwards a service-to-service bearer
//! token (issued by the platform IdP) under the standard
//! `Authorization: Bearer …` header. v1 reads the token from the
//! environment-supplied `PERSON_SERVICE_BEARER` value; the follow-up
//! ticket layers SPIFFE-mTLS on top.
//!
//! D14 fail-closed: timeouts and transport errors bubble up to the
//! caller. The submit use case treats a transport error as a 5xx; it
//! does NOT silently admit the submission.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use crate::application::port::{PersonRegistryError, PersonRegistryPort};

pub struct PersonRegistryHttpAdapter {
    base_url: String,
    bearer: SecretString,
    http: reqwest::Client,
}

impl PersonRegistryHttpAdapter {
    pub fn new(
        base_url: impl Into<String>,
        bearer: SecretString,
    ) -> Result<Self, PersonRegistryError> {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| PersonRegistryError::Transport(e.to_string()))?;
        Ok(Self {
            base_url,
            bearer,
            http,
        })
    }
}

#[async_trait]
impl PersonRegistryPort for PersonRegistryHttpAdapter {
    async fn exists(
        &self,
        person_id: Uuid,
    ) -> Result<bool, PersonRegistryError> {
        let url = format!("{}/v1/persons/{}", self.base_url, person_id);
        let bearer = self.bearer.expose_secret().to_string();
        let resp = self
            .http
            .get(&url)
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|e| PersonRegistryError::Transport(e.to_string()))?;
        match resp.status() {
            s if s.is_success() => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            other => Err(PersonRegistryError::UnexpectedStatus(other.as_u16())),
        }
    }
}

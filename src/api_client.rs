use std::str::FromStr;

use hyperswitch_masking::Maskable;
#[cfg(feature = "external_key_manager")]
use hyperswitch_masking::PeekInterface;
use reqwest::{
    Response, StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};

#[cfg(feature = "external_key_manager")]
use crate::config::ExternalKeyManagerConfig;
use crate::{
    config::GlobalConfig,
    error::{self, ResultContainerExt},
};

pub type Headers = std::collections::HashSet<(String, Maskable<String>)>;

pub(super) trait HeaderExt {
    fn construct_header_map(
        self,
    ) -> Result<HeaderMap, error::ContainerError<error::ApiClientError>>;
}

impl HeaderExt for Headers {
    fn construct_header_map(
        self,
    ) -> Result<HeaderMap, error::ContainerError<error::ApiClientError>> {
        self.into_iter().try_fold(
            HeaderMap::new(),
            |mut header_map, (header_name, header_value)| {
                let header_name = HeaderName::from_str(&header_name)
                    .change_error(error::ApiClientError::HeaderMapConstructionFailed)?;
                let header_value = header_value.into_inner();
                let header_value = HeaderValue::from_str(&header_value)
                    .change_error(error::ApiClientError::HeaderMapConstructionFailed)?;
                header_map.append(header_name, header_value);
                Ok(header_map)
            },
        )
    }
}

#[derive(Debug, Clone, Copy, strum::IntoStaticStr)]
#[strum(serialize_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
}

crate::impl_metric_value_from!(Method);

#[derive(Clone, serde::Deserialize, Debug)]
#[serde(default)]
pub struct ApiClientConfig {
    pub client_idle_timeout: u64,
    pub pool_max_idle_per_host: usize,
    // KMS encrypted
    #[cfg(feature = "external_key_manager")]
    pub identity: hyperswitch_masking::Secret<String>,
}

impl ApiClientConfig {
    #[cfg(feature = "external_key_manager")]
    pub fn validate_for_mtls(
        &self,
        external_key_manager_config: &ExternalKeyManagerConfig,
    ) -> Result<(), crate::error::ConfigurationError> {
        // Only validate if external key manager is enabled with mTLS
        if external_key_manager_config.is_mtls_enabled() && self.identity.peek().is_empty() {
            return Err(
                crate::error::ConfigurationError::InvalidConfigurationValueError(
                    "api_client.identity is required when mTLS is enabled".into(),
                ),
            );
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct ApiClient {
    pub inner: reqwest::Client,
}

impl std::ops::Deref for ApiClient {
    type Target = reqwest::Client;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl ApiClient {
    #[allow(unused_mut)]
    pub fn new(
        global_config: &GlobalConfig,
    ) -> Result<Self, error::ContainerError<error::ApiClientError>> {
        let mut client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .pool_idle_timeout(std::time::Duration::from_secs(
                global_config.api_client.client_idle_timeout,
            ))
            .pool_max_idle_per_host(global_config.api_client.pool_max_idle_per_host);

        #[cfg(feature = "external_key_manager")]
        {
            // mTLS-specific configuration
            if global_config.external_key_manager.is_mtls_enabled() {
                let client_identity =
                    reqwest::Identity::from_pem(global_config.api_client.identity.peek().as_ref())
                        .change_error(error::ApiClientError::IdentityParseFailed)?;

                let ca_cert = global_config
                    .external_key_manager
                    .get_ca_cert()
                    .ok_or_else(|| {
                        error::ApiClientError::MissingConfigurationError(
                            "CA certificate not configured for mTLS",
                        )
                    })?;

                let key_manager_ca_cert = reqwest::Certificate::from_pem(ca_cert.peek().as_ref())
                    .change_error(
                    error::ApiClientError::CertificateParseFailed {
                        service: "external_key_manager",
                    },
                )?;

                client = client
                    .use_rustls_tls()
                    .identity(client_identity)
                    .add_root_certificate(key_manager_ca_cert)
                    .https_only(true);
            }
        }

        let client = client
            .build()
            .change_error(error::ApiClientError::ClientConstructionFailed)?;

        Ok(Self { inner: client })
    }

    pub async fn send_request<T>(
        &self,
        purpose: &'static str,
        url: String,
        headers: Headers,
        method: Method,
        request_body: T,
    ) -> Result<ApiResponse, error::ContainerError<error::ApiClientError>>
    where
        T: serde::Serialize + Send + Sync + 'static,
    {
        let url =
            reqwest::Url::parse(&url).change_error(error::ApiClientError::UrlEncodingFailed)?;

        let host = url.host_str().unwrap_or("UNKNOWN").to_string();
        let headers = headers.construct_header_map()?;

        let request_builder = match method {
            Method::Get => self.get(url),
            Method::Post => self.post(url).json(&request_body),
        };

        crate::observability::metrics::EXTERNAL_HTTP_REQUEST_COUNT.add(
            1,
            crate::metric_attributes!(
                ("purpose", purpose),
                ("method", method),
                ("host", host.clone())
            ),
        );
        let start = std::time::Instant::now();

        let response = request_builder.headers(headers).send().await;

        let (outcome, status_code) = match response.as_ref() {
            Ok(resp) => {
                let status = resp.status();
                let outcome = match status.as_u16() {
                    200..=299 => "success",
                    300..=399 => "redirect",
                    400..=499 => "client_error",
                    500..=599 => "server_error",
                    _ => "unknown",
                };
                (outcome, Some(status.as_u16()))
            }
            Err(_) => ("transport_error", None),
        };

        let status_code = status_code
            .map(|s| s.to_string())
            .unwrap_or_else(|| "UNKNOWN".to_string());

        crate::observability::metrics::EXTERNAL_HTTP_REQUEST_DURATION.record(
            start.elapsed().as_secs_f64(),
            crate::metric_attributes!(
                ("purpose", purpose),
                ("method", method),
                ("host", host),
                ("outcome", outcome),
                ("status_code", status_code)
            ),
        );

        let response = response.change_error(error::ApiClientError::RequestNotSent)?;

        match response.status() {
            StatusCode::OK => Ok(ApiResponse(response)),
            StatusCode::INTERNAL_SERVER_ERROR => Err(error::ApiClientError::InternalServerError(
                response
                    .bytes()
                    .await
                    .change_error(error::ApiClientError::ResponseDecodingFailed)?,
            )
            .into()),
            StatusCode::BAD_REQUEST => Err(error::ApiClientError::BadRequest(
                response
                    .bytes()
                    .await
                    .change_error(error::ApiClientError::ResponseDecodingFailed)?,
            )
            .into()),
            StatusCode::UNAUTHORIZED => Err(error::ApiClientError::Unauthorized(
                response
                    .bytes()
                    .await
                    .change_error(error::ApiClientError::ResponseDecodingFailed)?,
            )
            .into()),
            _ => Err(error::ApiClientError::Unexpected {
                status_code: response.status(),
                message: response
                    .bytes()
                    .await
                    .change_error(error::ApiClientError::ResponseDecodingFailed)?,
            }
            .into()),
        }
    }
}

pub struct ApiResponse(Response);

impl ApiResponse {
    pub async fn deserialize_json<R, E>(self) -> Result<R, error::ContainerError<E>>
    where
        R: serde::de::DeserializeOwned,
        error::ContainerError<E>: From<error::ContainerError<error::ApiClientError>> + Send + Sync,
    {
        Ok(self
            .0
            .json::<R>()
            .await
            .change_error(error::ApiClientError::ResponseDecodingFailed)?)
    }
}

//! OpenFIGI client for ISIN-based asset reference lookups.

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;

const OPENFIGI_URL: &str = "https://api.openfigi.com/v3/mapping";

/// Asset metadata resolved from OpenFIGI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenFigiAssetMetadata {
    /// International Securities Identification Number.
    pub isin: String,
    /// Yahoo Finance ticker symbol, if a mapping can be inferred.
    pub yahoo_ticker: Option<String>,
    /// Human-readable security name.
    pub name: String,
    /// OpenFIGI-reported security type.
    pub security_type2: Option<String>,
    /// Trading currency, if present.
    pub currency: Option<String>,
    /// Exchange code used for the listing.
    pub exchange: Option<String>,
}

/// HTTP client for OpenFIGI mapping requests.
#[derive(Clone)]
pub struct OpenFigiClient {
    http: reqwest::Client,
    base_url: String,
}

impl OpenFigiClient {
    /// Build a client using environment-based configuration.
    ///
    /// `OPENFIGI_API_KEY` is optional and increases rate limits when present.
    /// `OPENFIGI_BASE_URL` is intended for tests and defaults to the public API.
    pub fn from_env() -> Result<Self, reqwest::Error> {
        let api_key = std::env::var("OPENFIGI_API_KEY").ok();
        let base_url =
            std::env::var("OPENFIGI_BASE_URL").unwrap_or_else(|_| OPENFIGI_URL.to_string());
        Self::new(base_url, api_key.as_deref())
    }

    /// Build a client with an explicit base URL and optional API key.
    pub fn new(base_url: impl Into<String>, api_key: Option<&str>) -> Result<Self, reqwest::Error> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        if let Some(api_key) = api_key
            && let Ok(value) = HeaderValue::from_str(api_key)
        {
            headers.insert("X-OPENFIGI-APIKEY", value);
        }

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            http,
            base_url: base_url.into(),
        })
    }

    /// Map an ISIN to a Yahoo-style ticker symbol.
    ///
    /// This method is fail-open by design: any transport or parsing issue is
    /// logged and converted to `None`.
    pub async fn lookup_yahoo_ticker(&self, isin: &str) -> Option<String> {
        self.lookup_asset_metadata(isin)
            .await
            .and_then(|asset| asset.yahoo_ticker)
    }

    /// Resolve OpenFIGI metadata for an ISIN.
    ///
    /// This method is fail-open by design: any transport or parsing issue is
    /// logged and converted to `None`.
    pub async fn lookup_asset_metadata(&self, isin: &str) -> Option<OpenFigiAssetMetadata> {
        let response = match self
            .http
            .post(&self.base_url)
            .json(&vec![OpenFigiMappingRequest {
                id_type: "ID_ISIN",
                id_value: isin,
            }])
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!("OpenFIGI request failed for ISIN {isin}: {error}");
                return None;
            }
        };

        let status = response.status();
        if !status.is_success() {
            tracing::warn!("OpenFIGI returned status {status} for ISIN {isin}");
            return None;
        }

        let payload = match response.json::<Vec<OpenFigiMappingResponse>>().await {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!("OpenFIGI response parsing failed for ISIN {isin}: {error}");
                return None;
            }
        };

        extract_asset_metadata(isin, &payload)
    }
}

#[derive(Debug, Serialize)]
struct OpenFigiMappingRequest<'a> {
    #[serde(rename = "idType")]
    id_type: &'a str,
    #[serde(rename = "idValue")]
    id_value: &'a str,
}

use serde::Serialize;

#[derive(Debug, Deserialize)]
struct OpenFigiMappingResponse {
    data: Option<Vec<OpenFigiInstrument>>,
}

#[derive(Debug, Deserialize)]
struct OpenFigiInstrument {
    name: Option<String>,
    ticker: Option<String>,
    #[serde(rename = "compositeFIGI")]
    _composite_figi: Option<String>,
    #[serde(rename = "compositeTicker")]
    composite_ticker: Option<String>,
    #[serde(rename = "exchCode")]
    exch_code: Option<String>,
    currency: Option<String>,
    security_type2: Option<String>,
}

fn extract_asset_metadata(
    isin: &str,
    payload: &[OpenFigiMappingResponse],
) -> Option<OpenFigiAssetMetadata> {
    let instrument = payload.first()?.data.as_ref()?.iter().find(|instrument| {
        instrument
            .security_type2
            .as_deref()
            .is_none_or(|kind| !kind.eq_ignore_ascii_case("MUTUAL FUND"))
    })?;

    let ticker = instrument
        .composite_ticker
        .as_deref()
        .or(instrument.ticker.as_deref())?
        .trim();

    if ticker.is_empty() {
        return None;
    }

    let suffix = instrument.exch_code.as_deref().and_then(exchange_suffix);

    let yahoo_ticker = Some(match suffix {
        Some(suffix) if !ticker.ends_with(suffix) => format!("{ticker}{suffix}"),
        _ => ticker.to_string(),
    });

    Some(OpenFigiAssetMetadata {
        isin: isin.to_string(),
        yahoo_ticker,
        name: instrument.name.clone().unwrap_or_else(|| isin.to_string()),
        security_type2: instrument.security_type2.clone(),
        currency: instrument.currency.clone(),
        exchange: instrument.exch_code.clone(),
    })
}

fn exchange_suffix(exchange: &str) -> Option<&'static str> {
    match exchange {
        "LN" => Some(".L"),
        "GY" => Some(".DE"),
        "GR" => Some(".DE"),
        "FP" => Some(".PA"),
        "NA" => Some(".AS"),
        "IM" => Some(".MI"),
        "BB" => Some(".BR"),
        "SM" => Some(".MC"),
        "SW" => Some(".SW"),
        "VX" => Some(".SW"),
        "SS" => Some(".ST"),
        "FH" => Some(".HE"),
        "ID" => Some(".IR"),
        "PL" => Some(".LS"),
        "NO" => Some(".OL"),
        "DC" => Some(".CO"),
        "AV" => Some(".VI"),
        "US" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenFigiMappingResponse, extract_asset_metadata};

    #[test]
    fn test_extract_yahoo_ticker_uses_composite_ticker_and_suffix() {
        let payload = serde_json::from_str::<Vec<OpenFigiMappingResponse>>(
            r#"[{"data":[{"ticker":"VWCE","compositeTicker":"VWCE","exchCode":"IM"}]}]"#,
        )
        .expect("payload should deserialize");

        let asset = extract_asset_metadata("IE00BK5BQT80", &payload).expect("asset should exist");
        assert_eq!(asset.yahoo_ticker.as_deref(), Some("VWCE.MI"));
    }

    #[test]
    fn test_extract_yahoo_ticker_returns_none_for_empty_data() {
        let payload = serde_json::from_str::<Vec<OpenFigiMappingResponse>>(r#"[{"data":[]}]"#)
            .expect("payload should deserialize");

        assert_eq!(extract_asset_metadata("IE00BK5BQT80", &payload), None);
    }
}

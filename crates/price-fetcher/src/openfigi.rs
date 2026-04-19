//! OpenFIGI client for ISIN-to-ticker lookup.

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;

const OPENFIGI_URL: &str = "https://api.openfigi.com/v3/mapping";

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

        extract_yahoo_ticker(&payload)
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
    ticker: Option<String>,
    #[serde(rename = "compositeFIGI")]
    _composite_figi: Option<String>,
    #[serde(rename = "compositeTicker")]
    composite_ticker: Option<String>,
    #[serde(rename = "exchCode")]
    exch_code: Option<String>,
    security_type2: Option<String>,
}

fn extract_yahoo_ticker(payload: &[OpenFigiMappingResponse]) -> Option<String> {
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

    Some(match suffix {
        Some(suffix) if !ticker.ends_with(suffix) => format!("{ticker}{suffix}"),
        _ => ticker.to_string(),
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
    use super::{OpenFigiMappingResponse, extract_yahoo_ticker};

    #[test]
    fn test_extract_yahoo_ticker_uses_composite_ticker_and_suffix() {
        let payload = serde_json::from_str::<Vec<OpenFigiMappingResponse>>(
            r#"[{"data":[{"ticker":"VWCE","compositeTicker":"VWCE","exchCode":"IM"}]}]"#,
        )
        .expect("payload should deserialize");

        assert_eq!(extract_yahoo_ticker(&payload).as_deref(), Some("VWCE.MI"));
    }

    #[test]
    fn test_extract_yahoo_ticker_returns_none_for_empty_data() {
        let payload = serde_json::from_str::<Vec<OpenFigiMappingResponse>>(r#"[{"data":[]}]"#)
            .expect("payload should deserialize");

        assert_eq!(extract_yahoo_ticker(&payload), None);
    }
}

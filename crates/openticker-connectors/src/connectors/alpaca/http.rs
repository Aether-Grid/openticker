use crate::{ConnectorError, ConnectorKind, rate_limit_error, retry_after_header};
use reqwest::blocking::Response;
use serde::Deserialize;

pub(super) fn decode_json_response<T: for<'de> Deserialize<'de>>(
    response: Response,
    operation: &str,
) -> Result<T, ConnectorError> {
    let status = response.status();
    if !status.is_success() {
        let retry_after = retry_after_header(&response);
        let body = response
            .text()
            .unwrap_or_else(|_| "<response body unavailable>".to_owned());
        let detail = format!("{operation} request returned {status}: {body}");
        if let Some(error) = rate_limit_error(
            ConnectorKind::Alpaca,
            status,
            retry_after.as_deref(),
            detail.clone(),
        ) {
            return Err(error);
        }
        return Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Alpaca,
            detail,
        });
    }

    response
        .json::<T>()
        .map_err(|error| ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Alpaca,
            detail: format!("failed to decode {operation} response: {error}"),
        })
}

pub(super) fn decode_order_submission_json<T: for<'de> Deserialize<'de>>(
    response: Response,
    operation: &str,
) -> Result<T, ConnectorError> {
    let status = response.status();
    if !status.is_success() {
        let retry_after = retry_after_header(&response);
        let body = response
            .text()
            .unwrap_or_else(|_| "<response body unavailable>".to_owned());
        let detail = format!("{operation} request returned {status}: {body}");
        if let Some(error) = rate_limit_error(
            ConnectorKind::Alpaca,
            status,
            retry_after.as_deref(),
            detail.clone(),
        ) {
            return Err(error);
        }
        return Err(ConnectorError::OrderSubmission {
            kind: ConnectorKind::Alpaca,
            detail,
        });
    }

    response
        .json::<T>()
        .map_err(|error| ConnectorError::OrderSubmission {
            kind: ConnectorKind::Alpaca,
            detail: format!("failed to decode {operation} response: {error}"),
        })
}

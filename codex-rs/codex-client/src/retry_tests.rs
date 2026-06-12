use super::*;
use crate::error::TransportError;
use crate::request::Request;
use http::Method;
use http::StatusCode;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

fn retry_policy(max_attempts: u64) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        base_delay: Duration::ZERO,
        retry_on: RetryOn {
            retry_429: true,
            retry_5xx: true,
            retry_transport: true,
        },
    }
}

fn request() -> Request {
    Request::new(Method::GET, "https://example.com/v1/responses".to_string())
}

fn http_error(status: StatusCode) -> TransportError {
    TransportError::Http {
        status,
        url: None,
        headers: None,
        body: None,
    }
}

#[test]
fn retry_decision_respects_configured_error_classes() {
    let retry_on = RetryOn {
        retry_429: true,
        retry_5xx: true,
        retry_transport: true,
    };

    for err in [
        http_error(StatusCode::TOO_MANY_REQUESTS),
        http_error(StatusCode::INTERNAL_SERVER_ERROR),
        http_error(StatusCode::BAD_GATEWAY),
        TransportError::Timeout,
        TransportError::Network("connection reset".to_string()),
    ] {
        assert!(retry_on.should_retry(&err, 0, 1));
    }

    for err in [
        http_error(StatusCode::BAD_REQUEST),
        http_error(StatusCode::UNAUTHORIZED),
        TransportError::Build("invalid request".to_string()),
        TransportError::RetryLimit,
    ] {
        assert!(!retry_on.should_retry(&err, 0, 1));
    }
}

#[test]
fn retry_decision_respects_disabled_error_classes() {
    let retry_on = RetryOn {
        retry_429: false,
        retry_5xx: false,
        retry_transport: false,
    };

    for err in [
        http_error(StatusCode::TOO_MANY_REQUESTS),
        http_error(StatusCode::BAD_GATEWAY),
        TransportError::Timeout,
        TransportError::Network("connection reset".to_string()),
    ] {
        assert!(!retry_on.should_retry(&err, 0, 1));
    }
}

#[test]
fn retry_decision_stops_after_configured_attempts() {
    let retry_on = RetryOn {
        retry_429: true,
        retry_5xx: true,
        retry_transport: true,
    };

    assert!(retry_on.should_retry(&http_error(StatusCode::BAD_GATEWAY), 0, 1));
    assert!(!retry_on.should_retry(&http_error(StatusCode::BAD_GATEWAY), 1, 1));
}

#[test]
fn backoff_never_exceeds_max_retry_delay() {
    assert_eq!(
        MAX_RETRY_DELAY,
        backoff(Duration::from_secs(20 * 60), /*attempt*/ 0)
    );
    assert_eq!(MAX_RETRY_DELAY, backoff(MAX_RETRY_DELAY, /*attempt*/ 8));
}

#[tokio::test]
async fn run_with_retry_retries_until_configured_limit() {
    let attempts = Arc::new(AtomicU64::new(0));
    let attempts_for_op = Arc::clone(&attempts);

    let result = run_with_retry(retry_policy(3), request, move |_request, attempt| {
        let attempts_for_op = Arc::clone(&attempts_for_op);
        async move {
            let current = attempts_for_op.fetch_add(1, Ordering::SeqCst);
            if current < 3 {
                Err(http_error(StatusCode::BAD_GATEWAY))
            } else {
                Ok(attempt)
            }
        }
    })
    .await
    .expect("retryable HTTP errors should retry within the configured limit");

    assert_eq!(3, result);
    assert_eq!(4, attempts.load(Ordering::SeqCst));
}

#[tokio::test]
async fn run_with_retry_returns_error_after_configured_limit() {
    let attempts = Arc::new(AtomicU64::new(0));
    let attempts_for_op = Arc::clone(&attempts);

    let err = run_with_retry(retry_policy(2), request, move |_request, _attempt| {
        let attempts_for_op = Arc::clone(&attempts_for_op);
        async move {
            attempts_for_op.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(http_error(StatusCode::BAD_GATEWAY))
        }
    })
    .await
    .expect_err("retryable HTTP errors should stop after the configured limit");

    assert!(matches!(
        err,
        TransportError::Http {
            status: StatusCode::BAD_GATEWAY,
            ..
        }
    ));
    assert_eq!(3, attempts.load(Ordering::SeqCst));
}

#[tokio::test]
async fn run_with_retry_does_not_retry_request_build_errors() {
    let attempts = Arc::new(AtomicU64::new(0));
    let attempts_for_op = Arc::clone(&attempts);

    let err = run_with_retry(retry_policy(3), request, move |_request, _attempt| {
        let attempts_for_op = Arc::clone(&attempts_for_op);
        async move {
            attempts_for_op.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(TransportError::Build("invalid request".to_string()))
        }
    })
    .await
    .expect_err("request build errors should not retry");

    assert!(matches!(err, TransportError::Build(message) if message == "invalid request"));
    assert_eq!(1, attempts.load(Ordering::SeqCst));
}

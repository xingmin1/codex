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

fn retry_policy_with_no_configured_retries() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 0,
        base_delay: Duration::ZERO,
        retry_on: RetryOn {
            retry_429: false,
            retry_5xx: false,
            retry_transport: false,
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
fn retry_decision_retries_all_http_network_and_timeout_errors() {
    let retry_on = RetryOn {
        retry_429: false,
        retry_5xx: false,
        retry_transport: false,
    };

    for err in [
        http_error(StatusCode::BAD_REQUEST),
        http_error(StatusCode::UNAUTHORIZED),
        http_error(StatusCode::TOO_MANY_REQUESTS),
        http_error(StatusCode::INTERNAL_SERVER_ERROR),
        TransportError::Timeout,
        TransportError::Network("connection reset".to_string()),
    ] {
        assert!(retry_on.should_retry(&err, 10, 0));
    }

    assert!(!retry_on.should_retry(&TransportError::Build("invalid request".to_string()), 0, 0));
    assert!(!retry_on.should_retry(&TransportError::RetryLimit, 0, 0));
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
async fn run_with_retry_retries_http_errors_past_configured_limit() {
    let attempts = Arc::new(AtomicU64::new(0));
    let attempts_for_op = Arc::clone(&attempts);

    let result = run_with_retry(
        retry_policy_with_no_configured_retries(),
        request,
        move |_request, attempt| {
            let attempts_for_op = Arc::clone(&attempts_for_op);
            async move {
                let current = attempts_for_op.fetch_add(1, Ordering::SeqCst);
                if current < 3 {
                    Err(http_error(StatusCode::BAD_REQUEST))
                } else {
                    Ok(attempt)
                }
            }
        },
    )
    .await
    .expect("HTTP errors should keep retrying until the request succeeds");

    assert_eq!(3, result);
    assert_eq!(4, attempts.load(Ordering::SeqCst));
}

#[tokio::test]
async fn run_with_retry_does_not_retry_request_build_errors() {
    let attempts = Arc::new(AtomicU64::new(0));
    let attempts_for_op = Arc::clone(&attempts);

    let err = run_with_retry(
        retry_policy_with_no_configured_retries(),
        request,
        move |_request, _attempt| {
            let attempts_for_op = Arc::clone(&attempts_for_op);
            async move {
                attempts_for_op.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(TransportError::Build("invalid request".to_string()))
            }
        },
    )
    .await
    .expect_err("request build errors should not retry");

    assert!(matches!(err, TransportError::Build(message) if message == "invalid request"));
    assert_eq!(1, attempts.load(Ordering::SeqCst));
}

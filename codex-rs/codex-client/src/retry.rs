use crate::error::TransportError;
use crate::request::Request;
use rand::Rng;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// 单次重试等待的最长时间。
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u64,
    pub base_delay: Duration,
    pub retry_on: RetryOn,
}

#[derive(Debug, Clone)]
pub struct RetryOn {
    pub retry_429: bool,
    pub retry_5xx: bool,
    pub retry_transport: bool,
}

impl RetryOn {
    /// 判断当前传输错误是否应该按策略继续重试。
    pub fn should_retry(&self, err: &TransportError, attempt: u64, max_attempts: u64) -> bool {
        if attempt >= max_attempts {
            return false;
        }

        match err {
            TransportError::Http { status, .. } => {
                (*status == http::StatusCode::TOO_MANY_REQUESTS && self.retry_429)
                    || (status.is_server_error() && self.retry_5xx)
            }
            TransportError::Timeout | TransportError::Network(_) => self.retry_transport,
            TransportError::Build(_) | TransportError::RetryLimit => false,
        }
    }
}

/// 计算指数退避等待时间，并限制到单次最大等待时间以内。
pub fn backoff(base: Duration, attempt: u64) -> Duration {
    if attempt == 0 {
        return base.min(MAX_RETRY_DELAY);
    }
    let exp = 2u64.saturating_pow(attempt.saturating_sub(1).min(u32::MAX as u64) as u32);
    let millis = base.as_millis() as u64;
    let raw = millis.saturating_mul(exp);
    let jitter: f64 = rand::rng().random_range(0.9..1.1);
    let delay = Duration::from_millis((raw as f64 * jitter) as u64);
    delay.min(MAX_RETRY_DELAY)
}

/// 按重试策略反复构造请求并执行操作，直到成功或遇到不可重试错误。
pub async fn run_with_retry<T, F, Fut>(
    policy: RetryPolicy,
    mut make_req: impl FnMut() -> Request,
    op: F,
) -> Result<T, TransportError>
where
    F: Fn(Request, u64) -> Fut,
    Fut: Future<Output = Result<T, TransportError>>,
{
    let mut attempt = 0;
    loop {
        let req = make_req();
        match op(req, attempt).await {
            Ok(resp) => return Ok(resp),
            Err(err)
                if policy
                    .retry_on
                    .should_retry(&err, attempt, policy.max_attempts) =>
            {
                let next_attempt = attempt.saturating_add(1);
                sleep(backoff(policy.base_delay, next_attempt)).await;
                attempt = next_attempt;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;

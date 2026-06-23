use super::*;
use codex_extension_api::ExtensionData;
use codex_extension_api::TurnItemContributor;
use codex_protocol::error::UnexpectedResponseError;
use codex_protocol::items::AgentMessageContent;
use http::StatusCode;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use std::time::Duration;

struct RewriteAgentMessageContributor;

impl TurnItemContributor for RewriteAgentMessageContributor {
    fn contribute<'a>(
        &'a self,
        _thread_store: &'a ExtensionData,
        _turn_store: &'a ExtensionData,
        item: &'a mut TurnItem,
    ) -> codex_extension_api::ExtensionFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let TurnItem::AgentMessage(agent_message) = item {
                agent_message.content = vec![AgentMessageContent::Text {
                    text: "plan contributed assistant text".to_string(),
                }];
            }
            Ok(())
        })
    }
}

fn assistant_output_text(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: Some("msg-1".to_string()),
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

#[tokio::test]
async fn plan_mode_uses_contributed_turn_item_for_last_agent_message() {
    let (mut session, turn_context) = crate::session::tests::make_session_and_context().await;
    let mut builder = codex_extension_api::ExtensionRegistryBuilder::new();
    builder.turn_item_contributor(Arc::new(RewriteAgentMessageContributor));
    session.services.extensions = Arc::new(builder.build());
    let turn_store = ExtensionData::new(turn_context.sub_id.clone());
    let mut state = PlanModeStreamState::new(&turn_context.sub_id);
    let mut last_agent_message = None;
    let item = assistant_output_text("original assistant text");

    let handled = handle_assistant_item_done_in_plan_mode(
        &session,
        &turn_context,
        &turn_store,
        &item,
        &mut state,
        /*previously_active_item*/ None,
        &mut last_agent_message,
    )
    .await;

    assert!(handled);
    assert_eq!(
        last_agent_message.as_deref(),
        Some("plan contributed assistant text")
    );
}

fn unexpected_status(status: StatusCode) -> CodexErr {
    CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status,
        body: String::new(),
        url: None,
        cf_ray: None,
        request_id: None,
        user_message: None,
        identity_authorization_error: None,
        identity_error_code: None,
    })
}

#[test]
fn persistent_sampling_retry_allows_only_recoverable_upstream_errors() {
    let retryable_errors = [
        CodexErr::Stream("stream closed before response.completed".to_string(), None),
        CodexErr::RequestTimeout,
        CodexErr::ServerOverloaded,
        CodexErr::InternalServerError,
        unexpected_status(StatusCode::BAD_GATEWAY),
        unexpected_status(StatusCode::GATEWAY_TIMEOUT),
        unexpected_status(StatusCode::REQUEST_TIMEOUT),
    ];

    for err in retryable_errors {
        assert!(
            is_persistent_sampling_retry_error(&err),
            "expected persistent retry for {err:?}"
        );
    }
}

#[test]
fn persistent_sampling_retry_rejects_bad_request_and_internal_errors() {
    let non_retryable_errors = [
        unexpected_status(StatusCode::BAD_REQUEST),
        unexpected_status(StatusCode::UNAUTHORIZED),
        CodexErr::InvalidRequest("failed to encode responses request".to_string()),
        CodexErr::InternalAgentDied,
        CodexErr::Json(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
    ];

    for err in non_retryable_errors {
        assert!(
            !is_persistent_sampling_retry_error(&err),
            "did not expect persistent retry for {err:?}"
        );
    }
}

#[test]
fn persistent_sampling_retry_delay_grows_and_is_capped() {
    let first_delay = persistent_sampling_retry_delay(1);
    let second_delay = persistent_sampling_retry_delay(2);
    let capped_delay = persistent_sampling_retry_delay(64);

    assert_eq!(Duration::from_secs(5), first_delay);
    assert!(second_delay > first_delay);
    assert_eq!(Duration::from_secs(10 * 60), capped_delay);
}

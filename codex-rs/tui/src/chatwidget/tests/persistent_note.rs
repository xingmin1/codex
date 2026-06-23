use super::*;
use codex_protocol::protocol::PersistentUserNoteState;
use codex_protocol::protocol::PersistentUserNoteStatus;
use codex_protocol::protocol::PersistentUserNoteUpdate;
use pretty_assertions::assert_eq;

fn configured_session_with_note(
    chat: &ChatWidget,
    note: PersistentUserNoteState,
) -> ThreadSessionState {
    let cwd = chat.config_ref().cwd.clone();
    ThreadSessionState {
        thread_id: ThreadId::new(),
        forked_from_id: None,
        fork_parent_title: None,
        thread_name: None,
        model: chat.current_model().to_string(),
        model_provider_id: chat.config_ref().model_provider_id.clone(),
        service_tier: chat.current_service_tier().map(str::to_string),
        approval_policy: AskForApproval::from(
            chat.config_ref().permissions.approval_policy.value(),
        ),
        approvals_reviewer: chat.config_ref().approvals_reviewer,
        permission_profile: chat.config_ref().permissions.permission_profile().clone(),
        active_permission_profile: chat.config_ref().permissions.active_permission_profile(),
        cwd: cwd.clone(),
        runtime_workspace_roots: chat.config_ref().workspace_roots.clone(),
        instruction_source_paths: Vec::new(),
        reasoning_effort: chat.current_reasoning_effort(),
        collaboration_mode: None,
        personality: chat.config_ref().personality,
        message_history: None,
        network_proxy: None,
        rollout_path: None,
        persistent_user_note: Some(note),
    }
}

fn submit_composer_text(chat: &mut ChatWidget, text: &str) {
    chat.bottom_pane
        .set_composer_text(text.to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn dispatch_composer_command(chat: &mut ChatWidget, text: &str) {
    chat.bottom_pane
        .set_composer_text(text.to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn next_persistent_note_update(
    op_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Op>,
) -> PersistentUserNoteUpdate {
    loop {
        let op = op_rx.try_recv().expect("expected persistent note op");
        if let Op::SetPersistentUserNote { update } = op {
            return update;
        }
    }
}

fn next_persistent_note_event(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
) -> PersistentUserNoteUpdate {
    loop {
        let event = rx.try_recv().expect("expected persistent note app event");
        if let AppEvent::SetPersistentUserNote { update } = event {
            return update;
        }
    }
}

#[tokio::test]
async fn note_slash_command_sets_multiline_text() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note remember q09\navoid extra read copies");

    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Set {
            text: "remember q09\navoid extra read copies".to_string(),
        }
    );
    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Note active"));
    assert!(rendered.contains("Text: remember q09"));
}

#[tokio::test]
async fn note_slash_command_supports_edit_clear_pause_resume() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note original note");
    submit_composer_text(&mut chat, "/note pause");
    submit_composer_text(&mut chat, "/note resume");
    submit_composer_text(&mut chat, "/note edit updated note");
    submit_composer_text(&mut chat, "/note clear");

    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Set {
            text: "original note".to_string(),
        }
    );
    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Pause
    );
    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Resume
    );
    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Edit {
            text: "updated note".to_string(),
        }
    );
    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Clear
    );
}

#[tokio::test]
async fn bare_note_slash_command_shows_current_note_details() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note remember q09");
    let _ = next_persistent_note_update(&mut op_rx);
    let _ = drain_insert_history(&mut rx);

    submit_composer_text(&mut chat, "/note");

    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Note"));
    assert!(rendered.contains("Status: active"));
    assert!(rendered.contains("remember q09"));
}

#[tokio::test]
async fn note_slash_command_uses_session_configured_note_after_resume() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.handle_thread_session(configured_session_with_note(
        &chat,
        PersistentUserNoteState {
            text: "resume note\nsecond line".to_string(),
            status: PersistentUserNoteStatus::Paused,
        },
    ));
    let _ = drain_insert_history(&mut rx);

    submit_composer_text(&mut chat, "/note");

    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Status: paused"));
    assert!(rendered.contains("resume note"));
    assert!(rendered.contains("second line"));
}

#[tokio::test]
async fn note_edit_slash_command_opens_editor_with_existing_note() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note initial note");
    let _ = next_persistent_note_update(&mut op_rx);
    let _ = drain_insert_history(&mut rx);

    dispatch_composer_command(&mut chat, "/note edit");
    chat.handle_paste(" plus edit".to_string());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_persistent_note_event(&mut rx) {
        PersistentUserNoteUpdate::Edit { text } => {
            assert_eq!(text, "initial note plus edit");
        }
        other => panic!("expected persistent note edit update, got {other:?}"),
    }
}

#[tokio::test]
async fn note_edit_slash_command_uses_session_configured_note_after_resume() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.handle_thread_session(configured_session_with_note(
        &chat,
        PersistentUserNoteState {
            text: "resume note".to_string(),
            status: PersistentUserNoteStatus::Active,
        },
    ));
    let _ = drain_insert_history(&mut rx);

    dispatch_composer_command(&mut chat, "/note edit");
    chat.handle_paste(" edited".to_string());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_persistent_note_event(&mut rx) {
        PersistentUserNoteUpdate::Edit { text } => {
            assert_eq!(text, "resume note edited");
        }
        other => panic!("expected persistent note edit update, got {other:?}"),
    }
}

#[tokio::test]
async fn note_slash_command_rejects_invalid_control_arguments() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note clear extra");

    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
}

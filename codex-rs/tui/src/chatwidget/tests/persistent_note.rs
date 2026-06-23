use super::*;
use codex_protocol::protocol::PersistentUserNoteUpdate;
use pretty_assertions::assert_eq;

fn submit_composer_text(chat: &mut ChatWidget, text: &str) {
    chat.bottom_pane
        .set_composer_text(text.to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
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

#[tokio::test]
async fn note_slash_command_sets_multiline_text() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note remember q09\navoid extra read copies");

    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Set {
            text: "remember q09\navoid extra read copies".to_string(),
        }
    );
}

#[tokio::test]
async fn note_slash_command_supports_edit_clear_pause_resume() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note edit updated note");
    submit_composer_text(&mut chat, "/note clear");
    submit_composer_text(&mut chat, "/note pause");
    submit_composer_text(&mut chat, "/note resume");

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
    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Pause
    );
    assert_eq!(
        next_persistent_note_update(&mut op_rx),
        PersistentUserNoteUpdate::Resume
    );
}

#[tokio::test]
async fn note_slash_command_rejects_invalid_control_arguments() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/note clear extra");

    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
}

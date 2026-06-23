//! `/note` 命令的持久 note 摘要和编辑 UI。

use super::*;
use codex_protocol::protocol::PersistentUserNoteState;
use codex_protocol::protocol::PersistentUserNoteStatus;
use codex_protocol::protocol::PersistentUserNoteUpdate;

impl ChatWidget {
    pub(crate) fn show_persistent_note_summary(&mut self) {
        self.add_plain_history_lines(persistent_note_summary_lines(
            self.current_persistent_note.as_ref(),
        ));
    }

    pub(crate) fn show_persistent_note_edit_prompt(&mut self) {
        let tx = self.app_event_tx.clone();
        let initial_text = self
            .current_persistent_note
            .as_ref()
            .map(|note| note.text.clone())
            .unwrap_or_default();
        let view = CustomPromptView::new(
            "Edit note".to_string(),
            "Type a persistent note and press Enter".to_string(),
            initial_text,
            /*context_label*/ None,
            Box::new(move |text: String| {
                tx.send(AppEvent::SetPersistentUserNote {
                    update: PersistentUserNoteUpdate::Edit { text },
                });
            }),
        );
        self.bottom_pane.show_view(Box::new(view));
    }

    pub(crate) fn set_persistent_note_from_ui(&mut self, update: PersistentUserNoteUpdate) {
        match self.apply_persistent_note_update_locally(&update) {
            Ok(state) => {
                self.submit_op(AppCommand::set_persistent_user_note(update));
                self.add_persistent_note_update_message(&state);
            }
            Err(message) => {
                self.add_error_message(message);
            }
        }
    }

    pub(crate) fn apply_persistent_note_update_locally(
        &mut self,
        update: &PersistentUserNoteUpdate,
    ) -> Result<PersistentUserNoteState, String> {
        let current = self.current_persistent_note.clone();
        let next = match update {
            PersistentUserNoteUpdate::Set { text } | PersistentUserNoteUpdate::Edit { text } => {
                let text = normalized_note_text(text)?;
                Some(PersistentUserNoteState {
                    text,
                    status: PersistentUserNoteStatus::Active,
                })
            }
            PersistentUserNoteUpdate::Clear => None,
            PersistentUserNoteUpdate::Pause => {
                let mut note = current.ok_or_else(|| "No note is currently set.".to_string())?;
                note.status = PersistentUserNoteStatus::Paused;
                Some(note)
            }
            PersistentUserNoteUpdate::Resume => {
                let mut note = current.ok_or_else(|| "No note is currently set.".to_string())?;
                note.status = PersistentUserNoteStatus::Active;
                Some(note)
            }
        };
        self.current_persistent_note = next.clone();
        Ok(next.unwrap_or_else(|| PersistentUserNoteState {
            text: String::new(),
            status: PersistentUserNoteStatus::Paused,
        }))
    }

    pub(crate) fn add_persistent_note_update_message(&mut self, state: &PersistentUserNoteState) {
        let title = if state.text.trim().is_empty() {
            "Note cleared".to_string()
        } else {
            format!("Note {}", persistent_note_status_label(&state.status))
        };
        let hint = if state.text.trim().is_empty() {
            None
        } else {
            Some(format!("Text: {}", first_note_line(&state.text)))
        };
        self.add_info_message(title, hint);
    }
}

fn persistent_note_summary_lines(note: Option<&PersistentUserNoteState>) -> Vec<Line<'static>> {
    let Some(note) = note.filter(|note| !note.text.trim().is_empty()) else {
        return vec![
            Line::from("Note".bold()),
            Line::from("No note is currently set."),
            Line::default(),
            Line::from("Commands: /note <text>, /note edit, /note pause, /note clear".dim()),
        ];
    };

    let command_hint = match note.status {
        PersistentUserNoteStatus::Active => "Commands: /note edit, /note pause, /note clear",
        PersistentUserNoteStatus::Paused => "Commands: /note edit, /note resume, /note clear",
    };
    let mut lines = vec![
        Line::from("Note".bold()),
        Line::from(vec![
            "Status: ".dim(),
            persistent_note_status_label(&note.status).into(),
        ]),
        Line::from("Text: ".dim()),
    ];
    lines.extend(note.text.lines().map(|line| Line::from(line.to_string())));
    lines.push(Line::default());
    lines.push(Line::from(command_hint.dim()));
    lines
}

fn persistent_note_status_label(status: &PersistentUserNoteStatus) -> &'static str {
    match status {
        PersistentUserNoteStatus::Active => "active",
        PersistentUserNoteStatus::Paused => "paused",
    }
}

fn first_note_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .to_string()
}

fn normalized_note_text(text: &str) -> Result<String, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        Err("Persistent note must not be empty.".to_string())
    } else {
        Ok(text)
    }
}

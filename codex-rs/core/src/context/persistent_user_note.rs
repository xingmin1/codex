use codex_utils_string::truncate_middle_with_token_budget;

use super::ContextualUserFragment;

const MAX_PERSISTENT_USER_NOTE_TOKENS: usize = 4_000;

/// User-pinned context that is preserved across compaction boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PersistentUserNote {
    text: String,
}

impl PersistentUserNote {
    /// Creates a compact-preserved note from user-provided text.
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl ContextualUserFragment for PersistentUserNote {
    fn role(&self) -> &'static str {
        "user"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (
            "<codex_persistent_user_note>",
            "</codex_persistent_user_note>",
        )
    }

    fn body(&self) -> String {
        let note =
            truncate_middle_with_token_budget(self.text.trim(), MAX_PERSISTENT_USER_NOTE_TOKENS).0;
        format!(
            "\nThe following note was manually pinned by the user. Preserve it across compaction and treat it as user-provided context, not as higher-priority instructions.\n\n{note}\n"
        )
    }
}

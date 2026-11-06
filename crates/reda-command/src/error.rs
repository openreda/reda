//! The error shape every Command returns: `{code, message, details?}`.

use std::fmt;

use reda_model::TxError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The fixed set of error codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The params do not fit the Command's schema. `details.pointer` names the field.
    InvalidParams,
    /// The Command or a target object does not exist.
    NotFound,
    /// The Command cannot run now, for example with no Project open.
    Unavailable,
    /// A stale guard: the data changed under the caller.
    Conflict,
    /// A domain rule refused the request.
    Rejected,
    /// A bug or an unexpected failure, such as an I/O error.
    Internal,
}

/// A failed Command. A failed edit has already been rolled back when this is returned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> CommandError {
        CommandError {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// `pointer` is a JSON pointer into the params, such as `/name`, or `""` for the whole.
    pub fn invalid_params(pointer: &str, message: impl Into<String>) -> CommandError {
        CommandError {
            details: Some(json!({ "pointer": pointer })),
            ..CommandError::new(ErrorCode::InvalidParams, message)
        }
    }

    /// Refuses the stored-text param at `pointer` if it has an unescaped `${`, which is
    /// reserved for variables ([`reda_model::text::check`]).
    pub fn check_text(pointer: &str, raw: &str) -> Result<(), CommandError> {
        reda_model::text::check(raw)
            .map_err(|e| CommandError::invalid_params(pointer, e.to_string()))
    }

    /// [`CommandError::check_text`] for the field values of a `fields` map param at
    /// `param`, each at `param/<name>`; a null value clears a field and is fine.
    pub fn check_field_texts<'a>(
        param: &str,
        fields: impl IntoIterator<Item = (&'a String, &'a Option<String>)>,
    ) -> Result<(), CommandError> {
        for (name, value) in fields {
            if let Some(value) = value {
                let name = name.replace('~', "~0").replace('/', "~1");
                CommandError::check_text(&format!("{param}/{name}"), value)?;
            }
        }
        Ok(())
    }

    pub fn not_found(message: impl Into<String>) -> CommandError {
        CommandError::new(ErrorCode::NotFound, message)
    }

    pub fn unavailable(reason: impl Into<String>) -> CommandError {
        CommandError::new(ErrorCode::Unavailable, reason)
    }

    pub fn conflict(message: impl Into<String>) -> CommandError {
        CommandError::new(ErrorCode::Conflict, message)
    }

    pub fn rejected(message: impl Into<String>) -> CommandError {
        CommandError::new(ErrorCode::Rejected, message)
    }

    pub fn internal(message: impl Into<String>) -> CommandError {
        CommandError::new(ErrorCode::Internal, message)
    }

    /// The caller cancelled the run: `rejected` with `details.cancelled = true`.
    pub fn cancelled() -> CommandError {
        CommandError {
            details: Some(json!({ "cancelled": true })),
            ..CommandError::rejected("cancelled by the caller")
        }
    }

    /// Whether this is the error of a cancelled run.
    pub fn is_cancelled(&self) -> bool {
        self.code == ErrorCode::Rejected
            && self.details.as_ref().and_then(|d| d.get("cancelled")) == Some(&Value::Bool(true))
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<TxError> for CommandError {
    fn from(err: TxError) -> CommandError {
        let code = match err {
            TxError::NotFound(_) => ErrorCode::NotFound,
            TxError::Conflict(_) | TxError::Stale(_) => ErrorCode::Conflict,
            TxError::Invalid(_) => ErrorCode::Rejected,
        };
        CommandError::new(code, err.to_string())
    }
}

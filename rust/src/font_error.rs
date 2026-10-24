//! Font failures retain their cause so compatibility fallbacks never hide limits
//! or cancellation. Display text remains suitable for the existing GUI errors.
use std::sync::atomic::AtomicBool;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Invalid,
    Unsupported,
    Unavailable,
    Limit,
    Cancelled,
}
#[derive(Clone, Debug)]
pub struct Error {
    kind: Kind,
    message: String,
}
impl Error {
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub(crate) fn new(kind: Kind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub(crate) fn limit(message: impl Into<String>) -> Self {
        Self::new(Kind::Limit, message)
    }
    pub(crate) fn unsupported(message: impl Into<String>) -> Self {
        Self::new(Kind::Unsupported, message)
    }
    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self::new(Kind::Unavailable, message)
    }
    pub(crate) fn check(cancel: &AtomicBool) -> Result<(), Self> {
        crate::input::check(cancel).map_err(|m| Self::new(Kind::Cancelled, m))
    }
    pub(crate) fn shape_metrics(self) -> Self {
        if self.kind == Kind::Invalid {
            Self::unsupported(self.message)
        } else {
            self
        }
    }
}
impl std::ops::Deref for Error {
    type Target = str;
    fn deref(&self) -> &str {
        &self.message
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}
impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::new(Kind::Invalid, s)
    }
}
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self::new(Kind::Invalid, s)
    }
}
impl From<Error> for String {
    fn from(e: Error) -> Self {
        e.message
    }
}
impl From<crate::input::ReadFailure> for Error {
    fn from(e: crate::input::ReadFailure) -> Self {
        match e {
            crate::input::ReadFailure::Io(s) => Self::unavailable(s),
            crate::input::ReadFailure::Limit(s) => Self::limit(s),
            crate::input::ReadFailure::Cancelled(s) => Self::new(Kind::Cancelled, s),
        }
    }
}

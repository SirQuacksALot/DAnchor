use std::fmt;

/// Errors from decoding a `DeviceInfo` out of a discovered service's TXT
/// record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxtParseError {
    MissingField(&'static str),
    InvalidValue { field: &'static str, value: String },
}

impl fmt::Display for TxtParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "TXT record missing field {field:?}"),
            Self::InvalidValue { field, value } => {
                write!(f, "TXT record field {field:?} has invalid value {value:?}")
            }
        }
    }
}

impl std::error::Error for TxtParseError {}

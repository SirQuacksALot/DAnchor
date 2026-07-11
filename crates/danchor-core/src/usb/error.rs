use std::fmt;

#[derive(Debug)]
pub enum UsbError {
    Io(std::io::Error),
    Plist(plist::Error),
    MissingField(&'static str),
    UnexpectedMessageType(String),
    /// usbmuxd replied to a `Connect`/`Listen` request with a non-zero
    /// `Result.Number`. usbmuxd's own header only documents a handful of
    /// these (e.g. 2 = BadDevice, 3 = ConnectionRefused); anything else is
    /// passed through as-is rather than guessed at.
    ConnectFailed(u64),
}

impl From<std::io::Error> for UsbError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<plist::Error> for UsbError {
    fn from(err: plist::Error) -> Self {
        Self::Plist(err)
    }
}

impl fmt::Display for UsbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "usbmuxd I/O error: {err}"),
            Self::Plist(err) => write!(f, "malformed usbmuxd plist message: {err}"),
            Self::MissingField(field) => write!(f, "usbmuxd message missing field {field:?}"),
            Self::UnexpectedMessageType(ty) => {
                write!(f, "unexpected usbmuxd MessageType {ty:?}")
            }
            Self::ConnectFailed(code) => {
                write!(f, "usbmuxd refused the connection (code {code})")
            }
        }
    }
}

impl std::error::Error for UsbError {}

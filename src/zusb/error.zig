pub const Error = error{
    Io,
    InvalidParam,
    Access,
    NoDevice,
    NotFound,
    Busy,
    Timeout,
    Overflow,
    Pipe,
    Interrupted,
    OutOfMemory,
    NotSupported,
    BadDescriptor,
    Other,
};
use std::error::Error;
//use std::fmt;
use std::io;
use std::num::{ParseFloatError, ParseIntError};
use std::result::Result as StdResult;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

use quick_xml::Error as QuickXmlError;
use rustc_hex::FromHexError;
use snafu::Snafu;

use crate::byte_buffer::ByteBufferError;
use crate::encoding_type::EncodingError;
use crate::node_types::StandardType;
use crate::reader::ReaderError;
use crate::sixbit::SixbitError;
use crate::value::Value;

pub type Result<T> = StdResult<T, KbinError>;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub enum KbinError {
    #[snafu(display("Unable to write {} header field", field))]
    HeaderWrite {
        field: &'static str,
        source: io::Error,
    },

    #[snafu(display("Invalid byte value for {} header field", field))]
    HeaderValue { field: &'static str },

    #[snafu(display("Unable to read {} bytes from data buffer", size))]
    DataRead { size: usize, source: io::Error },

    #[snafu(display("Unable to write a {} to data buffer", node_type))]
    DataWrite {
        node_type: &'static str,
        source: io::Error,
    },

    #[snafu(display("Unable to read bytes or not enough data read"))]
    DataConvert { source: io::Error },

    #[snafu(display("No node collection found"))]
    NoNodeCollection,

    #[snafu(display("Failed to interpret string as UTF-8"))]
    Utf8 { source: FromUtf8Error },

    #[snafu(display("Failed to interpret slice as UTF-8"))]
    Utf8Slice { source: Utf8Error },


    #[snafu(display(
        "Size Mismatch, type: {}, expected size: {}, actual size: {}",
        node_type,
        expected,
        actual
    ))]
    SizeMismatch {
        node_type: &'static str,
        expected: usize,
        actual: usize,
    },

    #[snafu(display("Unable to interpret input as {}", node_type))]
    StringParse {
        node_type: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },

    #[snafu(display("Unable to interpret integer input as {}", node_type))]
    StringParseInt {
        node_type: &'static str,
        source: ParseIntError,
    },

    #[snafu(display("Unable to interpret float input as {}", node_type))]
    StringParseFloat {
        node_type: &'static str,
        source: ParseFloatError,
    },

    #[snafu(display("Unable to convert from hexadecimal"))]
    HexError { source: FromHexError },

    #[snafu(display("Type mismatch, expected: {}, found: {}", expected, found))]
    TypeMismatch {
        expected: StandardType,
        found: StandardType,
    },

    #[snafu(display("Value mismatch, expected {}, but found {:?}", node_type, value))]
    ValueTypeMismatch {
        node_type: StandardType,
        value: Value,
    },

    #[snafu(display("Value mismatch, expected an array, but found {:?}", value))]
    ExpectedValueArray { value: Value },

    #[snafu(display("Invalid input for boolean: {}", input))]
    InvalidBooleanInput { input: u8 },

    #[snafu(display("Invalid node type for operation: {:?}", node_type))]
    InvalidNodeType { node_type: StandardType },

    #[snafu(display("Invalid state"))]
    InvalidState,

    #[snafu(display("Failed to handle byte buffer operation"))]
    ByteBuffer {
        #[snafu(backtrace)]
        source: ByteBufferError,
    },

    #[snafu(display("Failed to handle string encoding operation"))]
    Encoding {
        #[snafu(backtrace)]
        source: EncodingError,
    },

    #[snafu(display("Failed to read binary XML"))]
    Reader {
        #[snafu(backtrace)]
        source: ReaderError,
    },

    #[snafu(display("Failed to handle sixbit string operation"))]
    Sixbit {
        #[snafu(backtrace)]
        source: SixbitError,
    },

    #[snafu(display("Error handling XML"))]
    XmlError { source: QuickXmlError },
}

impl From<FromUtf8Error> for KbinError {
    #[inline]
    fn from(source: FromUtf8Error) -> Self {
        KbinError::Utf8 { source }
    }
}

impl From<Utf8Error> for KbinError {
    #[inline]
    fn from(source: Utf8Error) -> Self {
        KbinError::Utf8Slice { source }
    }
}

impl From<ByteBufferError> for KbinError {
    #[inline]
    fn from(source: ByteBufferError) -> Self {
        KbinError::ByteBuffer { source }
    }
}

impl From<EncodingError> for KbinError {
    #[inline]
    fn from(source: EncodingError) -> Self {
        KbinError::Encoding { source }
    }
}

impl From<ReaderError> for KbinError {
    #[inline]
    fn from(source: ReaderError) -> Self {
        KbinError::Reader { source }
    }
}

impl From<SixbitError> for KbinError {
    #[inline]
    fn from(source: SixbitError) -> Self {
        KbinError::Sixbit { source }
    }
}

impl From<QuickXmlError> for KbinError {
    #[inline]
    fn from(source: QuickXmlError) -> Self {
        KbinError::XmlError { source }
    }
}

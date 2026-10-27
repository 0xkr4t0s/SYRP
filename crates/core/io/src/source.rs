//! [`InputSource`]: something that can open fresh [`InputStream`]s on demand.

use crate::{InputStream, IoError};

/// A source that opens new streams on its data, and on data next to it: JUCE's `InputSource`.
///
/// Every call opens an independent stream that starts at position 0, so a reader can go back to
/// the start, or several readers can work at once. Files (WP-1.1-04) and URLs (WP-1.1-09)
/// implement it; any type can. The trait is dyn-compatible.
///
/// ```
/// use syrp_io::{InputSource, InputStream, IoError, IoErrorKind, MemoryInputStream};
///
/// /// A source over static bytes, with one neighbour called "next".
/// struct Static(&'static [u8]);
///
/// impl InputSource for Static {
///     fn create_input_stream(&self) -> Result<Box<dyn InputStream + Send>, IoError> {
///         Ok(Box::new(MemoryInputStream::new(self.0)))
///     }
///     fn create_input_stream_for(
///         &self,
///         relative_path: &str,
///     ) -> Result<Box<dyn InputStream + Send>, IoError> {
///         match relative_path {
///             "next" => Ok(Box::new(MemoryInputStream::new(b"neighbour"))),
///             _ => Err(IoError::new(IoErrorKind::Io)),
///         }
///     }
///     fn hash_code(&self) -> u64 {
///         self.0.len() as u64
///     }
/// }
///
/// let source: &dyn InputSource = &Static(b"data");
/// assert_eq!(source.create_input_stream()?.read_entire_stream_as_string()?, "data");
/// let mut next = source.create_input_stream_for("next")?;
/// assert_eq!(next.read_entire_stream_as_string()?, "neighbour");
/// assert!(source.create_input_stream_for("missing").is_err());
/// # Ok::<(), IoError>(())
/// ```
#[doc(alias = "juce::InputSource")]
pub trait InputSource {
    /// Opens a new stream on this source's data, at position 0.
    ///
    /// ```
    /// # use syrp_io::{InputSource, InputStream, IoError, MemoryInputStream};
    /// # struct S;
    /// # impl InputSource for S {
    /// #     fn create_input_stream(&self) -> Result<Box<dyn InputStream + Send>, IoError> {
    /// #         Ok(Box::new(MemoryInputStream::new(b"ab")))
    /// #     }
    /// #     fn create_input_stream_for(&self, _: &str) -> Result<Box<dyn InputStream + Send>, IoError> {
    /// #         self.create_input_stream()
    /// #     }
    /// #     fn hash_code(&self) -> u64 { 0 }
    /// # }
    /// let mut first = S.create_input_stream()?;
    /// first.read_u8()?;
    /// assert_eq!(S.create_input_stream()?.position(), 0);
    /// # Ok::<(), IoError>(())
    /// ```
    #[doc(alias = "createInputStream")]
    fn create_input_stream(&self) -> Result<Box<dyn InputStream + Send>, IoError>;

    /// Opens a new stream on data at `relative_path` from this source, such as a file next to
    /// it. Fails when there is nothing there.
    ///
    /// ```
    /// # use syrp_io::{InputSource, InputStream, IoError, IoErrorKind, MemoryInputStream};
    /// # struct S;
    /// # impl InputSource for S {
    /// #     fn create_input_stream(&self) -> Result<Box<dyn InputStream + Send>, IoError> {
    /// #         Ok(Box::new(MemoryInputStream::new(b"")))
    /// #     }
    /// #     fn create_input_stream_for(&self, _: &str) -> Result<Box<dyn InputStream + Send>, IoError> {
    /// #         Err(IoError::new(IoErrorKind::Io))
    /// #     }
    /// #     fn hash_code(&self) -> u64 { 0 }
    /// # }
    /// assert!(S.create_input_stream_for("other.wav").is_err());
    /// ```
    #[doc(alias = "createInputStreamFor")]
    fn create_input_stream_for(
        &self,
        relative_path: &str,
    ) -> Result<Box<dyn InputStream + Send>, IoError>;

    /// A hash that identifies the data, for example to key a cache: two sources over the same
    /// data give the same value.
    ///
    /// ```
    /// # use syrp_io::{InputSource, InputStream, IoError, MemoryInputStream};
    /// # struct S(u64);
    /// # impl InputSource for S {
    /// #     fn create_input_stream(&self) -> Result<Box<dyn InputStream + Send>, IoError> {
    /// #         Ok(Box::new(MemoryInputStream::new(b"")))
    /// #     }
    /// #     fn create_input_stream_for(&self, _: &str) -> Result<Box<dyn InputStream + Send>, IoError> {
    /// #         self.create_input_stream()
    /// #     }
    /// #     fn hash_code(&self) -> u64 { self.0 }
    /// # }
    /// assert_eq!(S(7).hash_code(), S(7).hash_code());
    /// ```
    #[doc(alias = "hashCode")]
    fn hash_code(&self) -> u64;
}

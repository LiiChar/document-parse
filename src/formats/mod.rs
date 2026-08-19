#[cfg(feature = "txt")]
pub mod txt;

#[cfg(feature = "markdown")]
pub mod markdown;

#[cfg(feature = "html")]
pub mod html;

#[cfg(feature = "rtf")]
pub mod rtf;

#[cfg(feature = "fb2")]
pub mod fb2;

#[cfg(feature = "epub")]
pub mod epub;

#[cfg(feature = "cbz")]
pub mod cbz;

#[cfg(feature = "docx")]
pub mod docx;

#[cfg(feature = "mobi")]
pub mod mobi;

#[cfg(feature = "pdf")]
pub mod pdf;

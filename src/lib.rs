pub mod error;
pub mod model;
pub mod parser;

mod formats;
mod utils;

pub use error::Error;
pub use model::{Chapter, ChapterContent, Content, Document, Metadata};
pub use parser::{ContentType, DocumentParser, ImageLoadType, Loader, ParseOptions};

pub mod prelude {
    pub use crate::{ContentType, DocumentParser, ImageLoadType, Loader, ParseOptions};
}

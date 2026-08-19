#[derive(Debug, Clone)]
pub struct Document {
    pub metadata: Metadata,
    pub content: Content,
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub cover: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Content {
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Clone)]
pub enum ChapterContent {
    Text(String),
    Html(String)
}

impl ChapterContent {
    pub fn content (&self) -> String {
        match self {
            ChapterContent::Html(s) => s.clone(),
            ChapterContent::Text(s) => s.clone()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chapter {
    pub title: Option<String>,
    pub content: ChapterContent,
}

#[derive(Debug, Clone)]
pub struct RawDocument {
    pub metadata: RawMetadata,
    pub chapters: Vec<RawChapter>,
    pub resources: Vec<RawResource>,
}

#[derive(Debug, Clone)]
pub struct RawMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub cover_id: Option<String>
}

#[derive(Debug, Clone)]
pub struct RawResource {
    pub id: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}


#[derive(Debug, Clone)]
pub struct RawChapter {
    pub title: Option<String>,
    pub content: String,
}

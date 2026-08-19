# Document-parser

> A pure-Rust document parsing and normalization library for books, ebooks, and text-based documents.

`Document-parser` is a Rust library for parsing different document formats into a unified representation.

The library is designed around a simple idea:

```text
Document format
      │
      ▼
    Loader
      │
      ▼
 RawDocument
      │
      ▼
  Finalization
      │
      ▼
   Document
```

Each format-specific loader is responsible only for understanding its source format and converting it into a common intermediate representation. The finalization pipeline then handles HTML/text conversion, sanitization, language detection, resource processing, cover extraction, and other format-independent operations.

This architecture makes the parser suitable for desktop, mobile, CLI, server, and embedded applications.

---

## Features

* Pure Rust parsing pipeline
* Unified document model
* HTML as the common intermediate representation
* Optional conversion to plain text
* HTML sanitization
* Language detection
* Chapter extraction
* Embedded image/resource handling
* Cover extraction
* Stable document identifiers
* Pluggable format loaders
* Configurable Cargo features
* No external executables or runtime conversion tools
* Designed for cross-platform applications, including mobile targets

### Supported formats

| Format            | Feature    | Status    |
| ----------------- | ---------- | --------- |
| TXT               | `txt`      | Supported |
| Markdown          | `markdown` | Supported |
| HTML              | `html`     | Supported |
| RTF               | `rtf`      | Supported |
| FB2               | `fb2`      | Supported |
| EPUB              | `epub`     | Supported |
| CBZ               | `cbz`      | Supported |
| DOCX              | `docx`     | Supported |
| MOBI / AZW / AZW3 | `mobi`     | Supported |
| PDF               | `pdf`      | Supported |

PDF support currently focuses on text extraction. Complex PDF layout reconstruction, multi-column analysis, scanned-document OCR, and advanced visual reconstruction are intentionally separate concerns.

---

## Installation

### All formats

By default, `document-parser` enables the `full` feature set.

```toml
[dependencies]
document-parser = "0.1"
```

### Only selected formats

You can disable default features and enable only what your application requires.

For example, an application focused on ebooks and PDF:

```toml
[dependencies]
document-parser = {
    version = "0.1",
    default-features = false,
    features = [
        "ebook",
        "pdf",
    ],
}
```

A minimal text/document parser:

```toml
[dependencies]
document-parser = {
    version = "0.1",
    default-features = false,
    features = [
        "txt",
        "markdown",
        "html",
    ],
}
```

---

## Cargo Features

### Default

```text
default = ["full"]
```

### Full

Enables all supported formats and the filesystem scanner.

```text
full
```

### Group features

```text
ebook
documents
```

A typical grouping is:

```text
ebook
├── epub
├── fb2
├── mobi
└── cbz

documents
├── txt
├── markdown
├── html
├── rtf
└── docx
```

PDF is intentionally kept separate:

```text
pdf
```

The scanner can also be enabled independently:

```text
scanner
```

Optional logging and serialization support can be enabled separately when available.

---

# Quick Start

```rust
use Document-parser::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parser = DocumentParser::new();

    let document = parser.parse("book.epub")?;

    println!("Title: {}", document.metadata.title);

    for chapter in document.content.chapters {
        match chapter.content {
            ChapterContent::Html(html) => {
                println!("{}", html);
            }

            ChapterContent::Text(text) => {
                println!("{}", text);
            }
        }
    }

    Ok(())
}
```

---

# Parser Configuration

Parsing behavior is controlled through `ParseOptions`.

```rust
use Document-parser::{
    DocumentParser,
    ParseOptions,
    ContentType,
    ImageLoadType,
};

let options = ParseOptions {
    content_type: ContentType::Html,
    image_load: ImageLoadType::Base64,
    detect_language: true,
    sanitize_html: true,
    split_txt_chapters: true,
    max_language_chars: 10_000,
    normalize_whitespace: true,
};

let document = DocumentParser::new()
    .with_options(options)
    .parse("book.epub")?;
```

## `ContentType`

Controls the representation of the final chapter content.

### HTML

```rust
ContentType::Html
```

Produces:

```rust
ChapterContent::Html(...)
```

### Text

```rust
ContentType::Text
```

Produces:

```rust
ChapterContent::Text(...)
```

Importantly, loaders themselves always produce HTML. Conversion to plain text happens during finalization.

---

# Images and Resources

The parser separates resource extraction from resource representation.

A loader extracts raw resources:

```rust
RawResource {
    id: String,
    mime_type: String,
    data: Vec<u8>,
}
```

The finalization stage decides how those resources should be represented.

## None

```rust
ImageLoadType::None
```

Images are omitted from the final document.

## Base64

```rust
ImageLoadType::Base64
```

Images are converted into data URLs:

```html
<img src="data:image/png;base64,..." />
```

## Paths

```rust
ImageLoadType::Paths
```

Resources are written to disk and referenced by their generated paths.

This separation is especially useful for EPUB, FB2, DOCX and CBZ.

---

# Document Model

The final representation is intentionally format-independent.

```rust
pub struct Document {
    pub metadata: Metadata,
    pub content: Content,
}
```

## Metadata

```rust
pub struct Metadata {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub cover: Option<Vec<u8>>,
}
```

## Content

```rust
pub struct Content {
    pub chapters: Vec<Chapter>,
}
```

## Chapter

```rust
pub struct Chapter {
    pub title: Option<String>,
    pub content: ChapterContent,
}
```

## ChapterContent

```rust
pub enum ChapterContent {
    Text(String),
    Html(String),
}
```

---

# Raw Document Model

Format-specific loaders do not directly construct `Document`.

Instead they produce:

```rust
pub struct RawDocument {
    pub metadata: RawMetadata,
    pub chapters: Vec<RawChapter>,
    pub resources: Vec<RawResource>,
}
```

Metadata:

```rust
pub struct RawMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub cover_id: Option<String>,
}
```

Chapter:

```rust
pub struct RawChapter {
    pub title: Option<String>,
    pub content: String,
}
```

Resource:

```rust
pub struct RawResource {
    pub id: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}
```

### Important invariant

`RawChapter.content` always contains HTML.

For example, a TXT loader may receive:

```text
Hello world.

This is a book.
```

and return:

```html
<p>Hello world.</p>
<p>This is a book.</p>
```

An EPUB loader may return:

```html
<p>Hello <strong>world</strong>.</p>
```

The finalizer then decides whether the application receives HTML or plain text.

---

# Architecture

The library follows a loader + normalization pipeline.

```text
                     ┌─────────────────┐
                     │ DocumentParser  │
                     └────────┬────────┘
                              │
                              ▼
                     ┌─────────────────┐
                     │ Loader registry │
                     └────────┬────────┘
                              │
             ┌────────────────┼────────────────┐
             │                │                │
             ▼                ▼                ▼
           TXT              EPUB              PDF
             │                │                │
             └────────────────┼────────────────┘
                              ▼
                       ┌─────────────┐
                       │ RawDocument │
                       └──────┬──────┘
                              │
                              ▼
                       ┌─────────────┐
                       │  Finalize   │
                       └──────┬──────┘
                              │
            ┌─────────────────┼─────────────────┐
            │                 │                 │
            ▼                 ▼                 ▼
         Content           Resources         Metadata
          Type              Images           Language
            │                 │                 │
            └─────────────────┼─────────────────┘
                              ▼
                         ┌──────────┐
                         │ Document │
                         └──────────┘
```

This separation is intentional.

A loader knows about its format.

The finalizer knows about application-independent normalization.

---

# Implementing a Custom Loader

Custom formats can be added without modifying the core parsing pipeline.

Implement `Loader`:

```rust
use std::path::Path;

use Document-parser::{
    Error,
    Loader,
    ParseOptions,
    RawChapter,
    RawDocument,
    RawMetadata,
};

pub struct MyLoader;

impl Loader for MyLoader {
    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("myformat")
            })
    }

    fn load(
        &self,
        path: &Path,
        _options: &ParseOptions,
    ) -> Result<RawDocument, Error> {
        Ok(RawDocument {
            metadata: RawMetadata {
                title: Some("My document".into()),
                author: None,
                description: None,
                language: None,
                cover_id: None,
            },

            chapters: vec![
                RawChapter {
                    title: None,
                    content: "<p>Hello world.</p>".into(),
                },
            ],

            resources: Vec::new(),
        })
    }
}
```

Register it:

```rust
let parser = DocumentParser::new()
    .register_loader(MyLoader);
```

---

# Error Handling

The public API uses a dedicated error type instead of exposing internal error implementations.

Typical errors include:

```rust
pub enum Error {
    UnsupportedFormat,
    Io(std::io::Error),
    InvalidDocument(String),
    Parser(String),
}
```

Example:

```rust
match DocumentParser::new().parse("book.epub") {
    Ok(document) => {
        println!(
            "Parsed: {}",
            document.metadata.title
        );
    }

    Err(error) => {
        eprintln!(
            "Failed to parse document: {}",
            error
        );
    }
}
```

---

# Format-specific Behavior

## TXT

The TXT loader:

* detects text encoding;
* reads plain text;
* can split text into chapters;
* converts text into HTML.

Chapter splitting is controlled by:

```rust
ParseOptions {
    split_txt_chapters: true,
    ..
}
```

---

## Markdown

Markdown is parsed with `pulldown-cmark`.

Common features include:

* headings;
* emphasis;
* strong text;
* lists;
* tables;
* task lists;
* strikethrough;
* footnotes.

The loader always returns HTML.

---

## HTML

HTML documents are loaded directly as HTML.

Metadata title extraction can use the document `<title>` and falls back to the file name when necessary.

Sanitization is performed during finalization rather than inside the loader.

---

## RTF

The RTF loader converts RTF formatting into HTML.

Basic formatting includes:

* bold;
* italic;
* underline;
* strikethrough;
* superscript;
* subscript;
* text colors.

Images and advanced RTF features may depend on the capabilities of the underlying RTF parser.

---

## FB2

FB2 support includes:

* title;
* author;
* annotation;
* language;
* sections;
* nested sections;
* images;
* cover references;
* links;
* poems;
* epigraphs;
* tables;
* basic inline formatting.

Embedded `<binary>` resources become `RawResource` entries.

---

## EPUB

EPUB support is based on:

```text
META-INF/container.xml
        ↓
OPF
        ↓
manifest
spine
metadata
resources
```

The loader handles:

* EPUB package discovery;
* OPF metadata;
* spine order;
* XHTML chapters;
* embedded images;
* cover identification;
* EPUB 2 cover metadata;
* EPUB 3 `cover-image` metadata.

Resources remain binary until the finalization stage.

---

## CBZ

CBZ archives are treated as ordered sequences of image pages.

Images are naturally sorted so that:

```text
1.jpg
2.jpg
10.jpg
```

is ordered numerically rather than lexicographically.

Each page becomes a chapter-like content unit:

```html
<img src="page-1" />
```

The first page is used as the cover candidate.

---

## DOCX

DOCX support uses the document structure provided by `docx-rust`.

The loader handles:

* paragraphs;
* headings;
* lists;
* numbered lists;
* hyperlinks;
* tables;
* text formatting;
* embedded images.

DOCX media files are exposed as raw resources instead of immediately being converted into Base64.

---

## MOBI / AZW / AZW3

The MOBI loader uses the HTML representation provided by the underlying MOBI parser.

It extracts:

* title;
* author;
* description;
* language;
* chapter-like pagebreak sections.

MOBI structure varies substantially between books, so chapter reconstruction is intentionally kept format-specific.

---

## PDF

PDF support is based on `pdf-extract`.

The current pipeline:

```text
PDF
 ↓
PDF text extraction
 ↓
page-level text
 ↓
normalization
 ↓
HTML paragraphs
```

This is intentionally a text-oriented PDF parser.

PDFs containing scanned pages require OCR and are outside the basic text extraction pipeline.

Complex layout reconstruction such as:

* multiple columns;
* advanced typography;
* visual reading order;
* headers and footers;
* tables;
* scanned pages;

may require a more advanced PDF analysis layer in the future.

---

# Security

HTML from external documents should not automatically be considered trusted.

By default, the library supports HTML sanitization through `ammonia`.

```rust
let options = ParseOptions {
    sanitize_html: true,
    ..
};
```

For applications displaying HTML inside a browser or WebView, sanitization should generally remain enabled unless the application has its own trusted HTML pipeline.

---

# Language Detection

Language detection is performed after the document has been normalized.

```rust
ParseOptions {
    detect_language: true,
    max_language_chars: 10_000,
    ..
}
```

The language detector operates on a limited amount of text so that very large documents do not require processing the entire book merely to determine its language.

---

# Testing

The project uses several levels of testing.

## Unit tests

Used for:

* text normalization;
* HTML conversion;
* encoding;
* metadata extraction;
* resource handling;
* chapter splitting.

## Integration tests

Each built-in loader is tested against representative documents.

## Fixtures

Binary formats use fixtures such as:

```text
tests/fixtures/
├── fb2/
├── epub/
├── cbz/
├── docx/
├── mobi/
└── pdf/
```

## Snapshot tests

HTML output can be validated using snapshot tests with `insta`.

This is especially useful for complex formats such as EPUB, FB2, DOCX and PDF.

Run tests:

```bash
cargo test --all-features
```

Run Clippy:

```bash
cargo clippy --all-features --all-targets -- -D warnings
```

Check formatting:

```bash
cargo fmt --all -- --check
```

---

# Feature Matrix

A consumer that only needs a subset of formats can minimize dependencies.

For example:

```toml
[dependencies]
Document-parser = {
    version = "0.1",
    default-features = false,
    features = [
        "txt",
        "markdown",
        "epub",
    ],
}
```

This allows applications to avoid compiling parsers that they do not need.

This is particularly useful for:

* mobile applications;
* WebAssembly projects;
* CLI utilities;
* embedded applications.

---

# Why HTML Is the Intermediate Representation

The library deliberately uses HTML as the common intermediate representation.

For example:

```text
TXT
 └──> HTML

Markdown
 └──> HTML

RTF
 └──> HTML

FB2
 └──> HTML

EPUB
 └──> HTML

DOCX
 └──> HTML

MOBI
 └──> HTML

PDF
 └──> HTML
```

This provides a common language for formatting:

```html
<strong>Bold</strong>
<em>Italic</em>
<p>Paragraph</p>
<h2>Heading</h2>
<table>...</table>
<img src="...">
```

The application can then choose:

```text
HTML
```

or:

```text
HTML → plain text
```

without forcing every loader to implement both representations independently.

---

# Design Principles

`Document-parser` follows several architectural principles.

### Format-specific logic stays inside loaders

An EPUB loader knows EPUB.

A DOCX loader knows DOCX.

The parser core does not.

### Loaders do not own application state

They do not manage:

* reading progress;
* database records;
* timestamps;
* UI state;
* user preferences.

### Resource extraction is separated from resource representation

Loaders expose raw resources.

The finalization stage determines whether they become:

* Base64;
* filesystem paths;
* nothing.

### The public API is format-independent

The application consumes:

```rust
Document
```

rather than:

```rust
EpubBook
Fb2Book
MobiBook
PdfBook
```

---

# Roadmap

Planned and possible improvements include:

* richer PDF layout reconstruction;
* better PDF paragraph detection;
* PDF multi-column detection;
* PDF heading detection;
* scanned PDF OCR integration;
* improved DOCX relationship/resource handling;
* more robust MOBI chapter detection;
* richer EPUB CSS processing;
* additional metadata extraction;
* more format fixtures and snapshot coverage;
* improved resource path handling;
* asynchronous parsing APIs where useful;
* additional ebook formats.

---

# Non-goals

`Document-parser` is not intended to be:

* a full document editor;
* a PDF renderer;
* an EPUB reader UI;
* a database;
* an OCR engine;
* a browser;
* a replacement for a complete PDF rendering engine.

Its primary responsibility is:

```text
document → structured content
```

---

# Example Architecture in an Application

A typical reader application can use `Document-parser` as the parsing layer:

```text
                   Application
                       │
                 ┌─────┴─────┐
                 │           │
               UI          Storage
                 │           │
                 └─────┬─────┘
                       │
                       ▼
              ┌─────────────────┐
              │ Document-parser │
              └─────┬───────────┘
                       │
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
      EPUB            FB2            PDF
        │              │              │
        └──────────────┼──────────────┘
                       ▼
                  Document
```

This allows the application to keep parsing concerns isolated from its storage, UI and business logic.

---

# License

This project is licensed under either of:

* [MIT License](LICENSE-MIT)
* [Apache License 2.0](LICENSE-APACHE)

at your option.

---

# Status

`Document-parser` is currently under active development.

The public architecture is designed around the following stable concepts:

```text
Document
Metadata
Content
Chapter
ChapterContent
RawDocument
RawMetadata
RawChapter
RawResource
Loader
DocumentParser
ParseOptions
```

The format implementations and advanced normalization logic may continue to evolve before the first stable `1.0.0` release.

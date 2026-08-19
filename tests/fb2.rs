// use document_parser::DocumentParser;


// #[test]
// fn fb2_extracts_metadata() {
//     let data =
//         include_bytes!(
//             "fixtures/fb2/book_with_cover.fb2"
//         );

//     let file =
//         TestFile::new(
//             "book.fb2",
//             data,
//         );

//     let document =
//         DocumentParser::new()
//             .parse(&file.path)
//             .expect("failed to parse");

//     assert!(
//         !document.metadata.title.is_empty()
//     );

//     assert!(
//         document.metadata.author.is_some()
//     );

//     assert!(
//         !document.content.chapters.is_empty()
//     );
// }


// #[test]
// fn fb2_extracts_images() {
//     let data =
//         include_bytes!(
//             "fixtures/fb2/book_with_images.fb2"
//         );

//     let file =
//         TestFile::new(
//             "book.fb2",
//             data,
//         );

//     let options =
//         ParseOptions {
//             image_load: ImageLoadType::Base64,
//             ..Default::default()
//         };

//     let document =
//         DocumentParser::new()
//             .with_options(options)
//             .parse(&file.path)
//             .expect("failed to parse");

//     let html =
//         document.content.chapters
//             .iter()
//             .filter_map(|chapter| {
//                 match &chapter.content {
//                     ChapterContent::Html(html) =>
//                         Some(html.as_str()),

//                     _ => None,
//                 }
//             })
//             .collect::<String>();

//     assert!(
//         html.contains("data:image/")
//     );
// }

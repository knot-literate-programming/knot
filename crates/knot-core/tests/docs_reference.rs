#![allow(missing_docs)]
//! The documentation's reference tables are generated from the code.

#[test]
fn chunk_options_page_embeds_the_generated_reference() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/book/src/chunk-options.md"
    );
    let page = std::fs::read_to_string(path).unwrap();
    let begin = page
        .find("<!-- BEGIN CHUNK OPTIONS")
        .expect("the page has a BEGIN CHUNK OPTIONS marker");
    let start = begin + page[begin..].find("-->\n").unwrap() + 4;
    let end = page
        .find("<!-- END CHUNK OPTIONS -->")
        .expect("the page has an END CHUNK OPTIONS marker");
    let expected = knot_core::parser::chunk_options_reference();
    assert_eq!(
        &page[start..end],
        expected,
        "docs/book/src/chunk-options.md is out of date; replace its table with the output of `cargo run -p knot-core --example chunk_options_reference`"
    );
}

#![allow(missing_docs)]
//! R sessions resumed from snapshots behave like complete executions.

use knot_core::{Compiler, Document};
use std::fs;

#[test]
#[ignore = "requires R with jsonlite and digest"]
fn attached_packages_keep_their_order_after_a_restore() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("knot.toml"), "").unwrap();
    let path = root.path().join("main.knot");
    fs::write(&path, "").unwrap();
    let mut compiler = Compiler::new(&path).unwrap();
    // The search path decides which of two same-named functions is used.
    let source = |comment: &str| {
        format!(
            "```{{r}}\nsuppressMessages({{library(jsonlite); library(digest)}})\n```\n\n```{{r}}\n# {comment}\ncat(search()[2:3])\n```\n"
        )
    };
    let search_path = |output: String| {
        let start = output.rfind("```output\n").unwrap() + "```output\n".len();
        output[start..].lines().next().unwrap().to_string()
    };
    let compile = |compiler: &mut Compiler, text: String| {
        compiler
            .compile(&Document::parse(text), "main.knot")
            .unwrap()
    };
    let complete = search_path(compile(&mut compiler, source("first")));
    assert_eq!(complete, "package:digest package:jsonlite");
    // Editing the second chunk resumes from the first chunk's snapshot.
    let resumed = search_path(compile(&mut compiler, source("edited")));
    assert_eq!(resumed, complete);
}

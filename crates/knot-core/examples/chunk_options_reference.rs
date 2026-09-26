//! Print the chunk options reference table embedded in
//! `docs/book/src/chunk-options.md`.
fn main() {
    print!("{}", knot_core::parser::chunk_options_reference());
}

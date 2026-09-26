#![allow(missing_docs)]
//! Without the R packages Knot's helpers need, the R interpreter does not
//! start: results and errors could not be reported (#135).
//!
//! A separate test binary: it changes the environment of the process.
//! Run with: cargo test -p knot-core --test r_required_packages -- --ignored

use knot_core::executors::{LanguageExecutor, r::RExecutor};
use std::time::Duration;

#[test]
#[ignore]
fn r_does_not_start_without_jsonlite() {
    let empty = tempfile::tempdir().unwrap();
    // SAFETY: the only test of this binary; no other thread reads the environment.
    unsafe {
        std::env::set_var("R_LIBS_SITE", empty.path());
        std::env::set_var("R_LIBS_USER", empty.path());
    }
    let cache = tempfile::tempdir().unwrap();
    let mut executor = RExecutor::new(cache.path().to_path_buf(), Duration::from_secs(60)).unwrap();
    let error = executor.initialize().unwrap_err();
    assert_eq!(
        error.to_string(),
        knot_core::defaults::missing_r_packages_message(&["jsonlite"])
    );
}

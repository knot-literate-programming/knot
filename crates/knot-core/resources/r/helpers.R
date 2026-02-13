# Knot R Helpers - Internal utility functions

# Internal state
.knot_results <- list()
.knot_warnings <- list()
.knot_error <- NULL

.knot_clear_state <- function() {
  .knot_results <<- list()
  .knot_warnings <<- list()
  .knot_error <<- NULL
}

.knot_add_warning <- function(w) {
  # Build warning object without NULL fields: jsonlite serializes list elements
  # that are NULL as {} (empty object) rather than null, which breaks Rust
  # deserialization. Omitting the field entirely lets serde use its Option::None default.
  warn_obj <- list(message = as.character(w$message))
  if (!is.null(w$call)) {
    warn_obj$call <- deparse(w$call)[1]
  }
  .knot_warnings <<- c(.knot_warnings, list(warn_obj))
}

.get_base_dir <- function() {
  dir <- Sys.getenv("KNOT_CACHE_DIR")
  if (dir == "") {
    dir <- tempdir()
  }
  dir.create(dir, recursive = TRUE, showWarnings = FALSE)
  dir
}

.write_metadata <- function(metadata, type = "result") {
  meta_file <- Sys.getenv("KNOT_METADATA_FILE")
  if (meta_file == "") return(FALSE)

  if (!requireNamespace("jsonlite", quietly = TRUE)) {
    return(FALSE)
  }

  # Update internal state
  if (type == "result" && !is.null(metadata)) {
    .knot_results <<- c(.knot_results, list(metadata))
  } else if (type == "error") {
    .knot_error <<- metadata
  }

  # Prepare full metadata object
  data <- list(
    results = .knot_results,
    warnings = .knot_warnings
  )
  
  # Only include error if it exists
  if (!is.null(.knot_error)) {
    data$error <- .knot_error
  }

  # Write JSON with auto_unbox = TRUE so scalar fields become JSON scalars.
  # Vectors that must stay as arrays (traceback, warnings list) are wrapped
  # with as.list() at the call site before being stored in the data object.
  tryCatch({
    json_content <- jsonlite::toJSON(data, auto_unbox = TRUE, pretty = TRUE)
    writeLines(json_content, meta_file, useBytes = TRUE)
  }, error = function(e) {
    # If JSON fails, write a minimal error so Rust doesn't hang
    writeLines('{"results":[], "warnings":[]}', meta_file)
  })
  
  TRUE
}

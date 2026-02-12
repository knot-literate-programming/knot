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
  warn_obj <- list(
    message = as.character(w$message),
    call = if (!is.null(w$call)) deparse(w$call)[1] else NULL
  )
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

  # Write JSON without any unboxing to avoid "Tried to unbox" errors
  # We handle the array structures in Rust.
  tryCatch({
    json_content <- jsonlite::toJSON(data, auto_unbox = FALSE, pretty = TRUE)
    writeLines(json_content, meta_file, useBytes = TRUE)
  }, error = function(e) {
    # If JSON fails, write a minimal error so Rust doesn't hang
    writeLines('{"results":[], "warnings":[]}', meta_file)
  })
  
  TRUE
}

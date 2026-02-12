# Knot R Helpers - Internal utility functions

# Internal state
.knot_warnings <- list()

.knot_clear_state <- function() {
  .knot_warnings <<- list()
}

.knot_add_warning <- function(w) {
  # Capture message and line if possible
  warn_obj <- list(
    message = w$message,
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

  # Load existing metadata
  data <- list(results = list(), warnings = .knot_warnings)
  
  if (file.exists(meta_file)) {
    tryCatch({
      existing <- jsonlite::fromJSON(meta_file, simplifyVector = FALSE)
      if (is.list(existing)) {
        if ("results" %in% names(existing)) data$results <- existing$results
        if ("warnings" %in% names(existing)) data$warnings <- existing$warnings
        if ("error" %in% names(existing)) data$error <- existing$error
      }
    }, error = function(e) {})
  }

  # Update data based on type
  if (type == "result") {
    data$results <- c(data$results, list(metadata))
  } else if (type == "error") {
    data$error <- metadata
  }
  
  # Always ensure latest warnings are included
  data$warnings <- .knot_warnings

  jsonlite::write_json(data, meta_file, auto_unbox = TRUE, pretty = TRUE)
  TRUE
}

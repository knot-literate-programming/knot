# Knot R Helpers - Internal utility functions

.get_base_dir <- function() {
  dir <- Sys.getenv("KNOT_CACHE_DIR")
  if (dir == "") {
    dir <- tempdir()
  }
  dir.create(dir, recursive = TRUE, showWarnings = FALSE)
  dir
}

.write_metadata <- function(metadata) {
  meta_file <- Sys.getenv("KNOT_METADATA_FILE")
  if (meta_file == "") return(FALSE)

  existing <- list()
  if (file.exists(meta_file)) {
    tryCatch({
      existing <- jsonlite::fromJSON(meta_file)
    }, error = function(e) {})
  }

  # Append new metadata
  combined <- c(existing, list(metadata))
  jsonlite::write_json(combined, meta_file, auto_unbox = TRUE)
  TRUE
}

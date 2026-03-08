# Knot R Executor - Bridge functions between Rust and R
#
# This module contains wrapper functions called by the Rust executor.
# All executor-specific logic stays in R, keeping the Rust code simple.

setup_environment <- function(metadata_file, cache_dir, fig_width, fig_height, fig_dpi, fig_format) {
  # Configure environment variables for chunk execution.
  #
  # Called by Rust before executing each code chunk to set up the
  # side-channel and graphics parameters.
  #
  # Args:
  #   metadata_file: Path to the temporary JSON file for metadata communication
  #   cache_dir: Path to the cache directory for generated files
  #   fig_width: Default figure width in inches
  #   fig_height: Default figure height in inches
  #   fig_dpi: Default DPI for raster graphics
  #   fig_format: Default graphics format (svg, png, pdf)

  Sys.setenv(KNOT_METADATA_FILE = metadata_file)
  Sys.setenv(KNOT_CACHE_DIR = cache_dir)
  Sys.setenv(KNOT_FIG_WIDTH = as.character(fig_width))
  Sys.setenv(KNOT_FIG_HEIGHT = as.character(fig_height))
  Sys.setenv(KNOT_FIG_DPI = as.character(fig_dpi))
  Sys.setenv(KNOT_FIG_FORMAT = fig_format)
}

knot_main_loop <- function(boundary) {
  # Robust main loop for R code execution.
  #
  # This function reads code lines from stdin until it sees "END_EXEC".
  # It then executes the accumulated code in a tryCatch block.
  # Using readLines() prevents R from entering "continuation mode" if
  # the code has a syntax error like an unclosed quote.
  #
  # Args:
  #   boundary: The string to print to signal the end of execution.

  while (TRUE) {
    # Clear internal state for new chunk
    .knot_clear_state()

    # Read code block line-by-line until END_EXEC
    code_lines <- character()
    while (TRUE) {
      line <- tryCatch({
        readLines(con = stdin(), n = 1)
      }, error = function(e) {
        return(character())
      })
      
      if (length(line) == 0) return(invisible(NULL)) # EOF
      if (line == "END_EXEC") break
      code_lines <- c(code_lines, line)
    }

    code <- paste(code_lines, collapse = "\n")

    # Parse code first to catch syntax errors separately
    parsed <- tryCatch({
      parse(text = code)
    }, error = function(e) {
      err_obj <- list(message = e$message, traceback = as.list(as.character(sys.calls())))
      # Force update of .knot_error and write to disk
      .write_metadata(err_obj, type = "error")
      return(NULL)
    })

    if (is.null(parsed)) {
      # Signal boundary and continue to next block
      cat("\n", file = stdout())
      cat(boundary, "\n", sep = "", file = stdout())
      cat("\n", file = stderr())
      cat(boundary, "\n", sep = "", file = stderr())
      flush(stdout())
      flush(stderr())
      next
    }

    # Execute code in global scope with error handling
    tryCatch({
      withCallingHandlers({
        .knot_res <- withVisible(eval(parsed, envir = .GlobalEnv))
        if (.knot_res$visible) print(.knot_res$value)
        # Success write
        .write_metadata(NULL)
      }, warning = function(w) {
        .knot_add_warning(w)
        invokeRestart("muffleWarning")
      })
    }, error = function(e) {
      err_obj <- list(message = e$message)
      if (!is.null(e$call)) err_obj$call <- deparse(e$call)[1]
      err_obj$traceback <- as.list(as.character(sys.calls()))
      # Error write
      .write_metadata(err_obj, type = "error")
    }, finally = {
      # Always signal boundary markers
      cat("\n", file = stdout())
      cat(boundary, "\n", sep = "", file = stdout())
      cat("\n", file = stderr())
      cat(boundary, "\n", sep = "", file = stderr())
      flush(stdout())
      flush(stderr())
    })
  }
}

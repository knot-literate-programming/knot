# R/typst.R

#' Get the base directory for knot-generated files
#'
#' Priority:
#' 1. KNOT_CACHE_DIR environment variable
#' 2. tempdir() as fallback
.get_base_dir <- function() {
  cache_dir <- Sys.getenv("KNOT_CACHE_DIR", unset = NA)
  if (!is.na(cache_dir) && nzchar(cache_dir)) {
    if (!dir.exists(cache_dir)) {
      dir.create(cache_dir, recursive = TRUE, showWarnings = FALSE)
    }
    return(normalizePath(cache_dir))
  }
  return(tempdir())
}

#' Write metadata to side-channel (KNOT_METADATA_FILE)
#'
#' Appends metadata to the side-channel file if KNOT_METADATA_FILE is set.
#'
#' @param metadata A list representing metadata entry
#' @return TRUE if metadata was written, FALSE otherwise
.write_metadata <- function(metadata) {
  metadata_file <- Sys.getenv("KNOT_METADATA_FILE", unset = NA)

  if (!is.na(metadata_file) && nzchar(metadata_file)) {
    # Read existing metadata if file exists
    existing_metadata <- list()
    if (file.exists(metadata_file)) {
      existing_json <- tryCatch(
        readLines(metadata_file, warn = FALSE),
        error = function(e) character(0)
      )
      if (length(existing_json) > 0) {
        existing_metadata <- tryCatch(
          jsonlite::fromJSON(paste(existing_json, collapse = "\n"), simplifyVector = FALSE),
          error = function(e) list()
        )
      }
    }

    # Append new metadata
    updated_metadata <- c(existing_metadata, list(metadata))

    # Write back as JSON array
    json_output <- jsonlite::toJSON(updated_metadata, auto_unbox = TRUE, pretty = FALSE)
    writeLines(json_output, metadata_file)

    return(TRUE)
  }

  return(FALSE)
}

#' Convert R objects to Typst representations
#'
#' Generic function to convert R objects (data frames, plots, etc.)
#' to Typst-compatible output via side-channel or serialization markers.
#'
#' @param x An R object to convert
#' @param ... Additional arguments passed to methods
#' @export
typst <- function(x, ...) {
  UseMethod("typst")
}

#' @export
typst.default <- function(x, ...) {
  print(x)
}

#' Convert data.frame to Typst table
#'
#' Serializes a data frame to CSV format and communicates via side-channel.
#' If not in knot environment, prints the data frame normally.
#'
#' @param x A data.frame
#' @param row.names Logical: include row names in CSV?
#' @param ... Additional arguments passed to write.csv
#' @export
typst.data.frame <- function(x, row.names = FALSE, ...) {
  # Generate unique filename based on dataframe hash
  df_hash <- digest::digest(x, algo = "sha256")
  filename <- sprintf("data_%s.csv", substr(df_hash, 1, 16))
  filepath <- file.path(.get_base_dir(), filename)

  # Write CSV to temp file
  utils::write.csv(x, filepath, row.names = row.names, ...)

  # Normalize path for cross-platform compatibility
  filepath_normalized <- normalizePath(filepath)

  # Write metadata via side-channel
  metadata <- list(type = "dataframe", path = filepath_normalized)

  if (!.write_metadata(metadata)) {
    # Not in knot environment, print normally
    print(x)
  }

  invisible(x)
}

#' Convert ggplot2 plot to Typst image
#'
#' Saves a ggplot2 plot to a file and communicates via side-channel.
#' If not in knot environment, prints the plot normally.
#'
#' Dimensions are read from chunk options via environment variables set by knot:
#' - KNOT_FIG_WIDTH: figure width in inches
#' - KNOT_FIG_HEIGHT: figure height in inches
#' - KNOT_FIG_DPI: resolution in DPI
#' - KNOT_FIG_FORMAT: output format (svg, png, pdf)
#'
#' These can be overridden by explicitly passing arguments.
#'
#' @param x A ggplot2 object
#' @param width Plot width in inches (default: from KNOT_FIG_WIDTH or 7)
#' @param height Plot height in inches (default: from KNOT_FIG_HEIGHT or 5)
#' @param dpi Resolution in dots per inch (default: from KNOT_FIG_DPI or 300)
#' @param format Output format: "svg", "png", or "pdf" (default: from KNOT_FIG_FORMAT or "svg")
#' @param ... Additional arguments passed to ggsave
#' @export
typst.ggplot <- function(x, width = NULL, height = NULL, dpi = NULL, format = NULL, ...) {
  # Read defaults from environment variables (set by knot from chunk options)
  if (is.null(width)) {
    width <- as.numeric(Sys.getenv("KNOT_FIG_WIDTH", "7"))
  }
  if (is.null(height)) {
    height <- as.numeric(Sys.getenv("KNOT_FIG_HEIGHT", "5"))
  }
  if (is.null(dpi)) {
    dpi <- as.integer(Sys.getenv("KNOT_FIG_DPI", "300"))
  }
  if (is.null(format)) {
    format <- Sys.getenv("KNOT_FIG_FORMAT", "svg")
  }
  if (!requireNamespace("ggplot2", quietly = TRUE)) {
    stop("ggplot2 package required for typst.ggplot(). Please install it.")
  }

  # Generate unique filename based on plot object hash
  plot_hash <- digest::digest(x, algo = "sha256")
  filename <- sprintf("plot_%s.%s", substr(plot_hash, 1, 16), format)
  filepath <- file.path(.get_base_dir(), filename)

  # Save plot using ggsave
  ggplot2::ggsave(
    filename = filepath,
    plot = x,
    width = width,
    height = height,
    dpi = dpi,
    device = format,
    ...
  )

  # Normalize path for cross-platform compatibility
  filepath_normalized <- normalizePath(filepath)

  # Write metadata via side-channel
  metadata <- list(type = "plot", path = filepath_normalized, format = format)

  if (!.write_metadata(metadata)) {
    # Not in knot environment, print normally
    print(x)
  }

  invisible(x)
}


#' Convert recorded base R plot to Typst image
#'
#' Saves a recorded plot (from recordPlot()) to a file and communicates via side-channel.
#' If not in knot environment, prints the plot normally.
#'
#' Dimensions are read from chunk options via environment variables set by knot:
#' - KNOT_FIG_WIDTH: figure width in inches
#' - KNOT_FIG_HEIGHT: figure height in inches
#' - KNOT_FIG_DPI: resolution in DPI
#' - KNOT_FIG_FORMAT: output format (svg, png, pdf)
#'
#' These can be overridden by explicitly passing arguments.
#'
#' Usage:
#'   plot(1:10)
#'   lines(1:10 * 2, col = "red")
#'   p <- recordPlot()
#'   typst(p)
#'
#' @param x A recordedplot object (from recordPlot())
#' @param width Plot width in inches (default: from KNOT_FIG_WIDTH or 7)
#' @param height Plot height in inches (default: from KNOT_FIG_HEIGHT or 5)
#' @param dpi Resolution in dots per inch (default: from KNOT_FIG_DPI or 300)
#' @param format Output format: "svg", "png", or "pdf" (default: from KNOT_FIG_FORMAT or "svg")
#' @param ... Additional arguments (ignored)
#' @export
typst.recordedplot <- function(x, width = NULL, height = NULL, dpi = NULL, format = NULL, ...) {
  # Read defaults from environment variables (set by knot from chunk options)
  if (is.null(width)) {
    width <- as.numeric(Sys.getenv("KNOT_FIG_WIDTH", "7"))
  }
  if (is.null(height)) {
    height <- as.numeric(Sys.getenv("KNOT_FIG_HEIGHT", "5"))
  }
  if (is.null(dpi)) {
    dpi <- as.integer(Sys.getenv("KNOT_FIG_DPI", "300"))
  }
  if (is.null(format)) {
    format <- Sys.getenv("KNOT_FIG_FORMAT", "svg")
  }

  # Generate unique filename based on plot object hash
  plot_hash <- digest::digest(x, algo = "sha256")
  filename <- sprintf("plot_%s.%s", substr(plot_hash, 1, 16), format)
  filepath <- file.path(.get_base_dir(), filename)

  # Open device with correct dimensions
  if (format == "svg") {
    svg(filepath, width = width, height = height)
  } else if (format == "png") {
    png(filepath, width = width * dpi, height = height * dpi, res = dpi, units = "px")
  } else if (format == "pdf") {
    pdf(filepath, width = width, height = height)
  } else {
    stop("Unsupported format: ", format, ". Use 'svg', 'png', or 'pdf'.")
  }

  # Replay the recorded plot
  tryCatch({
    replayPlot(x)
  }, finally = {
    dev.off()
  })

  # Normalize path for cross-platform compatibility
  filepath_normalized <- normalizePath(filepath)

  # Write metadata via side-channel
  metadata <- list(type = "plot", path = filepath_normalized, format = format)

  if (!.write_metadata(metadata)) {
    # Not in knot environment, print normally
    print(x)
  }

  invisible(x)
}


#' Get the current plot
#'
#' Captures the current base R graphics plot using recordPlot().
#' This is a convenience wrapper for use with typst(), providing
#' a consistent interface with the Python knot.current_plot() function.
#'
#' The function checks if there is an active graphics device before
#' attempting to record the plot. If no device is active, it will
#' stop with an error message.
#'
#' @return A recordedplot object representing the current plot
#'
#' @examples
#' \dontrun{
#' # In a knot chunk:
#' plot(1:10, type = "l")
#' lines(1:10 * 2, col = "red")
#' points(5, 10, pch = 19)
#' typst(current_plot())  # Captures and renders the plot
#' }
#'
#' @seealso \code{\link{recordPlot}} for the underlying R function
#' @export
current_plot <- function() {
  # Check if there's an active plot device
  # dev.cur() returns 1 for the null device (no graphics)
  if (grDevices::dev.cur() == 1) {
    stop(
      "No active graphics device. Create a plot first.\n",
      "Example: plot(1:10)"
    )
  }

  # Capture and return the current plot
  grDevices::recordPlot()
}

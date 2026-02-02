# R/typst.R

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
  filepath <- file.path(tempdir(), filename)

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
  filepath <- file.path(tempdir(), filename)

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


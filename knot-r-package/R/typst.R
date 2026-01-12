# R/typst.R

#' Convert R objects to Typst representations
#'
#' Generic function to convert R objects (data frames, plots, etc.)
#' to Typst-compatible output via serialization markers.
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
#' Serializes a data frame to CSV format with a special marker
#' that knot recognizes and converts to a Typst table.
#'
#' @param x A data.frame
#' @param row.names Logical: include row names in CSV?
#' @param ... Additional arguments passed to write.csv
#' @export
typst.data.frame <- function(x, row.names = FALSE, ...) {
  # Capture CSV output as character vector
  csv_lines <- utils::capture.output(utils::write.csv(x, stdout(), row.names = row.names, ...))

  # Print marker followed by CSV content
  cat("__KNOT_SERIALIZED_CSV__\n")
  cat(csv_lines, sep = "\n")
  cat("\n")

  invisible(x)
}

#' Convert ggplot2 plot to Typst image
#'
#' Saves a ggplot2 plot to a file and marks it for inclusion in Typst output.
#'
#' @param x A ggplot2 object
#' @param width Plot width in inches (default: 7)
#' @param height Plot height in inches (default: 5)
#' @param dpi Resolution in dots per inch (default: 300)
#' @param format Output format: "svg", "png", or "pdf" (default: "svg")
#' @param ... Additional arguments passed to ggsave
#' @export
typst.ggplot <- function(x, width = 7, height = 5, dpi = 300, format = "svg", ...) {
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

  # Print marker with absolute path (knot will copy to cache)
  cat(sprintf("__KNOT_SERIALIZED_PLOT__\n%s\n", normalizePath(filepath)))

  invisible(x)
}


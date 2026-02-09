# Knot R Output Formatting

current_plot <- function() {
  # Capture current base R plot using recordPlot()
  if (grDevices::dev.cur() == 1) {
    stop("No active graphics device. Create a plot first.")
  }
  grDevices::recordPlot()
}

typst <- function(obj, ...) {
  UseMethod("typst")
}

typst.ggplot <- function(obj, ...) {
  .save_plot(obj, ...)
}

typst.recordedplot <- function(obj, ...) {
  .save_recordedplot(obj, ...)
}

typst.data.frame <- function(obj, ...) {
  .save_dataframe(obj, ...)
}

typst.default <- function(obj, ...) {
  print(obj)
  invisible(obj)
}

.save_plot <- function(plot_obj, width = NULL, height = NULL, dpi = NULL, format = NULL) {
  if (!requireNamespace("ggplot2", quietly = TRUE)) return(plot_obj)

  # Use provided values or fall back to environment variables
  width <- if (!is.null(width)) width else as.numeric(Sys.getenv("KNOT_FIG_WIDTH", "7"))
  height <- if (!is.null(height)) height else as.numeric(Sys.getenv("KNOT_FIG_HEIGHT", "5"))
  dpi <- if (!is.null(dpi)) dpi else as.integer(Sys.getenv("KNOT_FIG_DPI", "300"))
  format <- if (!is.null(format)) format else Sys.getenv("KNOT_FIG_FORMAT", "svg")

  # Create stable hash
  hash <- digest::digest(plot_obj, algo = "xxhash64")
  filename <- sprintf("plot_%s.%s", hash, format)
  filepath <- file.path(.get_base_dir(), filename)

  ggplot2::ggsave(filepath, plot = plot_obj, width = width, height = height, dpi = dpi, device = format)

  metadata <- list(
    type = "plot",
    path = normalizePath(filepath, mustWork = FALSE),
    format = format
  )

  if (!.write_metadata(metadata)) {
    print(plot_obj)
  }

  invisible(plot_obj)
}

.save_recordedplot <- function(plot_obj, width = NULL, height = NULL, dpi = NULL, format = NULL) {
  # Use provided values or fall back to environment variables
  width <- if (!is.null(width)) width else as.numeric(Sys.getenv("KNOT_FIG_WIDTH", "7"))
  height <- if (!is.null(height)) height else as.numeric(Sys.getenv("KNOT_FIG_HEIGHT", "5"))
  dpi <- if (!is.null(dpi)) dpi else as.integer(Sys.getenv("KNOT_FIG_DPI", "300"))
  format <- if (!is.null(format)) format else Sys.getenv("KNOT_FIG_FORMAT", "svg")

  # Create stable hash from the recorded plot
  hash <- digest::digest(plot_obj, algo = "xxhash64")
  filename <- sprintf("plot_%s.%s", hash, format)
  filepath <- file.path(.get_base_dir(), filename)

  # Open device based on format
  if (format == "svg") {
    grDevices::svg(filepath, width = width, height = height)
  } else if (format == "png") {
    grDevices::png(filepath, width = width * dpi, height = height * dpi, res = dpi)
  } else if (format == "pdf") {
    grDevices::pdf(filepath, width = width, height = height)
  } else {
    stop(sprintf("Unsupported format: %s", format))
  }

  # Replay the plot on the new device
  grDevices::replayPlot(plot_obj)
  grDevices::dev.off()

  metadata <- list(
    type = "plot",
    path = normalizePath(filepath, mustWork = FALSE),
    format = format
  )

  if (!.write_metadata(metadata)) {
    print(plot_obj)
  }

  invisible(plot_obj)
}

.save_dataframe <- function(df) {
  # Hash content
  hash <- digest::digest(df, algo = "xxhash64")
  filename <- sprintf("dataframe_%s.csv", hash)
  filepath <- file.path(.get_base_dir(), filename)

  write.csv(df, filepath, row.names = FALSE)

  metadata <- list(
    type = "dataframe",
    path = normalizePath(filepath, mustWork = FALSE)
  )

  if (!.write_metadata(metadata)) {
    print(df)
  }

  invisible(df)
}

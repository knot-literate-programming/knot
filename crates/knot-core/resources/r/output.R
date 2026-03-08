# Knot R Output Formatting

base_plot <- function(expr, width = NULL, height = NULL, dpi = NULL, format = NULL) {
  # Get parameters from env vars or use provided values
  width <- if (!is.null(width)) width else as.numeric(Sys.getenv("KNOT_FIG_WIDTH", "7"))
  height <- if (!is.null(height)) height else as.numeric(Sys.getenv("KNOT_FIG_HEIGHT", "5"))
  dpi <- if (!is.null(dpi)) dpi else as.integer(Sys.getenv("KNOT_FIG_DPI", "300"))
  format <- if (!is.null(format)) format else Sys.getenv("KNOT_FIG_FORMAT", "pdf")

  # Create file path
  hash <- digest::digest(paste(Sys.time(), runif(1)), algo = "xxhash64")
  filename <- sprintf("plot_%s.%s", hash, format)
  filepath <- file.path(.get_base_dir(), filename)

  # Open device
  if (format == "svg") {
    svglite::svglite(filepath, width = width, height = height)
  } else if (format == "png") {
    grDevices::png(filepath, width = width * dpi, height = height * dpi, res = dpi)
  } else if (format == "pdf") {
    grDevices::pdf(filepath, width = width, height = height)
  } else {
    stop(sprintf("Unsupported format: %s", format))
  }

  # Evaluate expression on this device
  tryCatch({
    eval(substitute(expr), envir = parent.frame())
  }, finally = {
    grDevices::dev.off()
  })

  metadata <- list(
    type   = "plot",
    path   = normalizePath(filepath, mustWork = FALSE),
    format = format
  )

  .write_metadata(metadata)

  invisible(filepath)
}

typst <- function(obj, ...) {
  UseMethod("typst")
}

typst.ggplot <- function(obj, ...) {
  .save_plot(obj, ...)
}

# Note: base R plots should use current_plot() function

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

  device <- if (format == "svg") svglite::svglite else format
  ggplot2::ggsave(filepath, plot = plot_obj, width = width, height = height, dpi = dpi, device = device)

  metadata <- list(
    type   = "plot",
    path   = normalizePath(filepath, mustWork = FALSE),
    format = format
  )

  if (!.write_metadata(metadata)) {
    print(plot_obj)
  }

  invisible(plot_obj)
}

.save_base_plot <- function(plot_obj, width = NULL, height = NULL, dpi = NULL, format = NULL) {
  # Extract parameters from plot_obj or use provided values or fall back to env vars
  width <- if (!is.null(width)) width else if (!is.null(plot_obj$width)) plot_obj$width else as.numeric(Sys.getenv("KNOT_FIG_WIDTH", "7"))
  height <- if (!is.null(height)) height else if (!is.null(plot_obj$height)) plot_obj$height else as.numeric(Sys.getenv("KNOT_FIG_HEIGHT", "5"))
  dpi <- if (!is.null(dpi)) dpi else if (!is.null(plot_obj$dpi)) plot_obj$dpi else as.integer(Sys.getenv("KNOT_FIG_DPI", "300"))
  format <- if (!is.null(format)) format else if (!is.null(plot_obj$format)) plot_obj$format else Sys.getenv("KNOT_FIG_FORMAT", "svg")

  # Create hash based on timestamp (can't easily hash the plot)
  hash <- digest::digest(paste(Sys.time(), runif(1)), algo = "xxhash64")
  filename <- sprintf("plot_%s.%s", hash, format)
  filepath <- file.path(.get_base_dir(), filename)

  # Copy current device to file
  if (format == "svg") {
    grDevices::dev.copy(svglite::svglite, file = filepath, width = width, height = height)
  } else if (format == "png") {
    grDevices::dev.copy(grDevices::png, file = filepath,
                       width = width * dpi, height = height * dpi, res = dpi)
  } else if (format == "pdf") {
    grDevices::dev.copy(grDevices::pdf, file = filepath, width = width, height = height)
  } else {
    stop(sprintf("Unsupported format: %s", format))
  }
  grDevices::dev.off()  # Close the copy device

  metadata <- list(
    type   = "plot",
    path   = normalizePath(filepath, mustWork = FALSE),
    format = format
  )

  if (!.write_metadata(metadata)) {
    warning("Could not write metadata, plot may not be included in document")
  }

  invisible(filepath)
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

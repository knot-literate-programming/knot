# R/to_typst.R

to_typst <- function(x, ...) {
  UseMethod("to_typst")
}

to_typst.default <- function(x, ...) {
  print(x)
}

to_typst.data.frame <- function(x, row.names = FALSE, ...) {
  # Capture CSV output as character vector
  csv_lines <- utils::capture.output(utils::write.csv(x, stdout(), row.names = row.names, ...))

  # Print marker followed by CSV content
  cat("__KNOT_SERIALIZED_CSV__\n")
  cat(csv_lines, sep = "\n")
  cat("\n")
}


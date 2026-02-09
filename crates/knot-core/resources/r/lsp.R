# Knot R LSP Support

get_hover <- function(topic) {
  tryCatch({
    h <- utils::help(topic)
    if (length(h) > 0) {
      tf <- tempfile()
      tools::Rd2txt(utils:::.getHelpFile(h), out = tf, options = list(underline_titles = FALSE))
      content <- readLines(tf)
      unlink(tf)
      paste(content, collapse = "\n")
    } else {
      ""
    }
  }, error = function(e) {
    ""
  })
}

get_completions <- function(token) {
  tryCatch({
    if (grepl("\\$", token)) {
      parts <- strsplit(token, "\\$")[[1]]
      obj_name <- parts[1]
      prefix <- if (length(parts) > 1) parts[2] else ""

      if (exists(obj_name, envir = .GlobalEnv)) {
        obj <- get(obj_name, envir = .GlobalEnv)
        if (is.list(obj) || is.data.frame(obj)) {
          n <- names(obj)
          matches <- n[startsWith(n, prefix)]
          paste(matches, collapse = "\n")
        } else {
          ""
        }
      } else {
        ""
      }
    } else {
      # Global completions
      matches <- utils::apropos(sprintf("^%s", token))
      paste(matches, collapse = "\n")
    }
  }, error = function(e) {
    ""
  })
}

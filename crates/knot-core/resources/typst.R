# Knot - R support for literate programming with Typst

# --- Session Management ---

.knot_save_session <- function(path) {
  tryCatch({
    # Save objects
    save.image(file = path)
    
    # Save loaded packages
    packages_path <- sub("\\.RData$", "_packages.rds", path)
    saveRDS(.packages(), packages_path)
    
    TRUE
  }, error = function(e) {
    message(sprintf("Error saving R session: %s", e$message))
    FALSE
  })
}

.knot_load_session <- function(path) {
  tryCatch({
    if (!file.exists(path)) return(FALSE)
    
    # Restore packages first
    packages_path <- sub("\\.RData$", "_packages.rds", path)
    if (file.exists(packages_path)) {
      pkgs <- readRDS(packages_path)
      # Suppress package startup messages
      invisible(lapply(pkgs, function(p) {
        tryCatch(library(p, character.only = TRUE), error = function(e) {})
      }))
    }
    
    # Restore objects into GlobalEnv
    load(file = path, envir = .GlobalEnv)
    
    TRUE
  }, error = function(e) {
    message(sprintf("Error loading R session: %s", e$message))
    FALSE
  })
}

# --- Constant Objects (Caching) ---

.knot_hash_object <- function(obj_name) {
  if (!requireNamespace("digest", quietly = TRUE)) {
    stop("Package 'digest' is required. Install with: install.packages('digest')")
  }
  
  if (!exists(obj_name, envir = .GlobalEnv)) {
    return("NONE")
  }
  
  obj <- get(obj_name, envir = .GlobalEnv)
  digest::digest(obj, algo = "xxhash64")
}

.knot_save_constant <- function(obj_name, path) {
  if (exists(obj_name, envir = .GlobalEnv)) {
    saveRDS(get(obj_name, envir = .GlobalEnv), file = path)
    TRUE
  } else {
    FALSE
  }
}

.knot_load_constant <- function(obj_name, path) {
  if (file.exists(path)) {
    assign(obj_name, readRDS(path), envir = .GlobalEnv)
    TRUE
  } else {
    FALSE
  }
}

# --- Output Formatting (Typst) ---

typst <- function(obj) {
  # Generic S3 method could be implemented here
  # For now, we handle basic types
  
  if (inherits(obj, "ggplot")) {
    return(.knot_save_plot(obj))
  } else if (is.data.frame(obj)) {
    return(.knot_save_dataframe(obj))
  }
  
  # Fallback
  print(obj)
  invisible(obj)
}

.knot_get_base_dir <- function() {
  dir <- Sys.getenv("KNOT_CACHE_DIR")
  if (dir == "") {
    dir <- tempdir()
  }
  dir.create(dir, recursive = TRUE, showWarnings = FALSE)
  dir
}

.knot_write_metadata <- function(metadata) {
  meta_file <- Sys.getenv("KNOT_METADATA_FILE")
  if (meta_file == "") return(FALSE)
  
  existing <- list()
  if (file.exists(meta_file)) {
    tryCatch({
      existing <- jsonlite::fromJSON(meta_file)
    }, error = function(e) {})
  }
  
  # Append new metadata (jsonlite handling of single object vs list is tricky, simplification here)
  # Ideally we append to the list
  combined <- c(existing, list(metadata))
  jsonlite::write_json(combined, meta_file, auto_unbox = TRUE)
  TRUE
}

.knot_save_plot <- function(plot_obj) {
  if (!requireNamespace("ggplot2", quietly = TRUE)) return(plot_obj)
  
  width <- as.numeric(Sys.getenv("KNOT_FIG_WIDTH", "7"))
  height <- as.numeric(Sys.getenv("KNOT_FIG_HEIGHT", "5"))
  dpi <- as.integer(Sys.getenv("KNOT_FIG_DPI", "300"))
  format <- Sys.getenv("KNOT_FIG_FORMAT", "svg")
  
  # Create stable hash
  # This is simplified; in production we might want to hash the object content
  # For now we use a random ID or rely on the chunk hash provided by Rust?
  # Rust provides execution context, but here we are inside R.
  # Let's use a random hash for the filename to match Python implementation style
  hash <- digest::digest(plot_obj, algo = "xxhash64")
  filename <- sprintf("plot_%s.%s", hash, format)
  filepath <- file.path(.knot_get_base_dir(), filename)
  
  ggplot2::ggsave(filepath, plot = plot_obj, width = width, height = height, dpi = dpi, device = format)
  
  metadata <- list(
    type = "plot",
    path = normalizePath(filepath, mustWork = FALSE),
    format = format
  )
  
  if (!.knot_write_metadata(metadata)) {
    print(plot_obj)
  }
  
  invisible(plot_obj)
}

.knot_save_dataframe <- function(df) {
  # Hash content
  hash <- digest::digest(df, algo = "xxhash64")
  filename <- sprintf("dataframe_%s.csv", hash)
  filepath <- file.path(.knot_get_base_dir(), filename)
  
  write.csv(df, filepath)
  
  metadata <- list(
    type = "dataframe",
    path = normalizePath(filepath, mustWork = FALSE)
  )
  
  if (!.knot_write_metadata(metadata)) {
    print(df)
  }
  
  invisible(df)
}

# --- LSP Support ---

.knot_get_hover <- function(topic) {
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

.knot_get_completions <- function(token) {
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
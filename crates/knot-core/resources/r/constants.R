# Knot R Constant Objects Management

hash_object <- function(obj_name) {
  if (!requireNamespace("digest", quietly = TRUE)) {
    stop("Package 'digest' is required. Install with: install.packages('digest')")
  }

  if (!exists(obj_name, envir = .GlobalEnv)) {
    return("NONE")
  }

  obj <- get(obj_name, envir = .GlobalEnv)
  digest::digest(obj, algo = "xxhash64")
}

save_constant <- function(obj_name, path) {
  if (exists(obj_name, envir = .GlobalEnv)) {
    saveRDS(get(obj_name, envir = .GlobalEnv), file = path)
    TRUE
  } else {
    FALSE
  }
}

load_constant <- function(obj_name, path) {
  if (file.exists(path)) {
    assign(obj_name, readRDS(path), envir = .GlobalEnv)
    TRUE
  } else {
    FALSE
  }
}

hash_objects_batch <- function(names) {
  results <- setNames(sapply(names, hash_object), names)
  cat(jsonlite::toJSON(results, auto_unbox = TRUE), "\n")
}

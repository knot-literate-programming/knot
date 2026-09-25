# Knot R Session Management

save_session <- function(path) {
  res <- tryCatch({
    # Save objects
    save.image(file = path)

    # Save loaded packages
    packages_path <- sub("\\.RData$", "_packages.rds", path)
    saveRDS(list(packages = .packages(), working_directory = getwd()), packages_path)

    TRUE
  }, error = function(e) {
    message(sprintf("Error saving R session: %s", e$message))
    FALSE
  })
  cat(as.character(res), "\n")
  res
}

load_session <- function(path) {
  res <- tryCatch({
    if (!file.exists(path)) return(FALSE)

    # Restore packages first
    packages_path <- sub("\\.RData$", "_packages.rds", path)
    if (file.exists(packages_path)) {
      context <- readRDS(packages_path)
      setwd(context$working_directory)
      # .packages() lists the most recently attached package first, and each
      # library() call goes first on the search path: attach in reverse, i.e.
      # in the original order, so that masking is the same as before saving.
      pkgs <- rev(context$packages)
      # Suppress package startup messages
      invisible(lapply(pkgs, function(p) {
        library(p, character.only = TRUE)
      }))
    }

    # Restore objects into GlobalEnv
    load(file = path, envir = .GlobalEnv)

    TRUE
  }, error = function(e) {
    message(sprintf("Error loading R session: %s", e$message))
    FALSE
  })
  cat(as.character(res), "\n")
  res
}

# Knot R Session Management

save_session <- function(path, excluded = character()) {
  res <- tryCatch({
    # Select bindings without removing or reloading objects in the live session.
    names_to_save <- setdiff(ls(envir = .GlobalEnv, all.names = TRUE), excluded)
    save(list = names_to_save, file = path, envir = .GlobalEnv)

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
      pkgs <- context$packages
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

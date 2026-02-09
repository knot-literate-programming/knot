# Knot R Session Management

save_session <- function(path) {
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

load_session <- function(path) {
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

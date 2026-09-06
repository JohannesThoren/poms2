fn main() {
    // `sqlx::migrate!` embeds every file under this directory at compile
    // time, but Cargo's own change detection only looks at tracked source
    // files (Cargo.toml, .rs files, etc.) - it has no idea the macro
    // depends on this directory's contents. Without this, adding or
    // editing a migration without also touching a .rs file (or wiping the
    // build cache) means cargo happily reuses the stale compiled
    // poms-db - and worse, Docker's BuildKit cache mount for the cargo
    // target dir persists across image builds too, so even a fresh
    // `docker compose build` can silently ship an old migration set. This
    // line makes cargo watch the whole directory so both cases rebuild
    // correctly.
    println!("cargo:rerun-if-changed=src/migrations");
}

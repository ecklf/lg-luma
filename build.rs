fn main() {
    println!("cargo:rerun-if-env-changed=LUMA_TMDB_API_KEY");
    let key = std::env::var("LUMA_TMDB_API_KEY").unwrap_or_default();
    let key: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(64)
        .collect();
    println!("cargo:rustc-env=LUMA_TMDB_API_KEY={key}");
}

//! Host-only preprocessing. Runtime never needs to blur bundled wallpapers.
#[path = "../../../../src/glass_backdrop.rs"]
mod glass_backdrop;

fn main() -> Result<(), String> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or("Expected wallpaper directory")?;
    let mut count = 0;
    for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().is_none_or(|extension| extension != "jpg")
            || path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().ends_with("@2x"))
        {
            continue;
        }
        let atlas = glass_backdrop::prepare(image::open(&path).map_err(|error| error.to_string())?);
        let output = glass_backdrop::sidecar_path(&path);
        atlas.save(&output).map_err(|error| error.to_string())?;
        // Validate the same dimensions and decoder limits the renderer will use.
        glass_backdrop::load(&output)?;
        count += 1;
    }
    println!("Prepared {count} lossless Liquid Glass wallpaper atlases.");
    Ok(())
}

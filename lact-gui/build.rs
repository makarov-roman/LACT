use lact_gui_build::{combine_css_files, extract_css_classes, generate_css_classes_module};
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    let dest_path = out_dir.join("combined.css");

    let (combined_css, css_files) = combine_css_files(Path::new("src"));

    for file in &css_files {
        println!("cargo:rerun-if-changed={}", file.display());
    }

    fs::write(&dest_path, &combined_css).expect("Could not write combined CSS file");
    fs::write(
        out_dir.join("css_classes.rs"),
        generate_css_classes_module(&extract_css_classes(&combined_css)),
    )
        .expect("Could not write CSS classes module");
}

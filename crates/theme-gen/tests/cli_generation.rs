use std::{fs, process::Command};

#[test]
fn generator_creates_css_files_for_catalog_and_clears_stale_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    let catalog_path = temp.path().join("theme-catalog.json");
    let output_dir = temp.path().join("themes");

    fs::create_dir_all(&output_dir).expect("output dir created");
    fs::write(output_dir.join("stale.css"), "stale").expect("stale file written");
    fs::write(
        &catalog_path,
        r##"[
  {
    "name": "tokyo-night",
    "display_name": "Tokyo Night",
    "author": "enkia",
    "tone": "dark",
    "split_surface": false,
    "palette": {
      "bg": "#1a1b26",
      "fg": "#c0caf5",
      "black": "#15161e",
      "red": "#f7768e",
      "green": "#9ece6a",
      "yellow": "#e0af68",
      "blue": "#7aa2f7",
      "magenta": "#bb9af7",
      "cyan": "#7dcfff",
      "white": "#a9b1d6"
    },
    "tags": ["cool", "vibrant"]
  },
  {
    "name": "manuscript",
    "display_name": "Manuscript",
    "author": "Notesmith",
    "tone": "dark",
    "split_surface": true,
    "palette": {
      "bg": "#191614",
      "fg": "#f5ede1",
      "black": "#11100f",
      "red": "#c96a5a",
      "green": "#768a53",
      "yellow": "#c7a15a",
      "blue": "#6e88b5",
      "magenta": "#9b79b5",
      "cyan": "#6ba5a8",
      "white": "#ede3d3"
    },
    "tags": ["paper", "split"]
  }
]"##,
    )
    .expect("catalog written");

    let output = Command::new(env!("CARGO_BIN_EXE_theme-gen"))
        .args([
            "--catalog",
            catalog_path.to_str().expect("catalog path utf8"),
            "--output",
            output_dir.to_str().expect("output path utf8"),
        ])
        .output()
        .expect("binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Generated 2 theme files"
    );
    assert!(!output_dir.join("stale.css").exists());

    let tokyo = fs::read_to_string(output_dir.join("tokyo-night.css")).expect("tokyo css exists");
    let manuscript =
        fs::read_to_string(output_dir.join("manuscript.css")).expect("manuscript css exists");

    assert!(tokyo.contains("[data-theme=\"tokyo-night\"][data-tone=\"dark\"]"));
    assert!(tokyo.contains("--neutral-11: #c0caf5;"));
    assert!(manuscript.contains("[data-theme=\"manuscript\"] .editor-surface"));
    assert!(manuscript.contains("--blue-11: #6e88b5;"));
}

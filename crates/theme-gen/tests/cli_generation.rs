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
    "name": "dark",
    "display_name": "Dark",
    "author": "Notesmith",
    "tone": "dark",
    "split_surface": false,
    "palette": {
      "bg": "#111316",
      "fg": "#f0f2f4",
      "black": "#0b0d0f",
      "red": "#d26a73",
      "green": "#72a878",
      "yellow": "#c9a15f",
      "blue": "#79a7ff",
      "magenta": "#a88bd4",
      "cyan": "#66b7bb",
      "white": "#f0f2f4"
    },
    "tags": ["neutral", "native"]
  },
  {
    "name": "split",
    "display_name": "Split",
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
    "editor_palette": {
      "bg": "#fbfbfa",
      "fg": "#252a31",
      "black": "#1b1f24",
      "red": "#b94f5a",
      "green": "#4e7f5c",
      "yellow": "#9a712a",
      "blue": "#4f83dc",
      "magenta": "#765aa8",
      "cyan": "#347e8c",
      "white": "#ffffff"
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

    let dark = fs::read_to_string(output_dir.join("dark.css")).expect("dark css exists");
    let split = fs::read_to_string(output_dir.join("split.css")).expect("split css exists");

    assert!(dark.contains("[data-theme=\"dark\"][data-tone=\"dark\"]"));
    assert!(dark.contains("--neutral-11: #f0f2f4;"));
    assert!(split.contains("[data-theme=\"split\"] .editor-surface"));
    assert!(split.contains("--neutral-0: #fbfbfa;"));
    assert!(split.contains("--blue-11: #4f83dc;"));
}

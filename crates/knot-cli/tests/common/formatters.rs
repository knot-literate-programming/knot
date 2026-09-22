use std::{fs, path::Path, process::Command, sync::OnceLock};

pub fn install_formatters(bin: &Path) {
    static FIXTURE: OnceLock<tempfile::TempDir> = OnceLock::new();
    let fixture = FIXTURE.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let result = Command::new("rustc")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/formatter.rs"
            ))
            .arg("-o")
            .arg(dir.path().join("formatter.exe"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        dir
    });
    fs::create_dir_all(bin).unwrap();
    for name in ["air", "ruff"] {
        let name = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        };
        fs::copy(fixture.path().join("formatter.exe"), bin.join(name)).unwrap();
    }
}

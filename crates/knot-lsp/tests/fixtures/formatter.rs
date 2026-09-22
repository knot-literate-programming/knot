// Portable subprocess fixture: deterministic failures and a controlled slow run.
use std::{fs, io::Read, path::PathBuf, time::{Duration, Instant}};
fn main() {
    let mut code = String::new();
    std::io::stdin().read_to_string(&mut code).unwrap();
    if code.contains("FAIL") {
        eprintln!("fixture syntax error");
        std::process::exit(2);
    }
    if let Some(path) = code.lines().find_map(|line| line.strip_prefix("# WAIT ")) {
        let path = PathBuf::from(path);
        fs::write(path.with_extension("started"), "ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !path.with_extension("release").exists() {
            assert!(Instant::now() < deadline, "test did not release formatter");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    print!("{}", code.replace("x=1", "x = 1"));
}

// Portable formatter subprocess used by format_command.rs (no R/Python needed).
use std::{fs, io::{Read, Write}};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args[0], "format");
    let python = args[1] == "-";
    let code = if python {
        assert_eq!(&args[2..], &["--stdin-filename", "chunk.py"]);
        let mut code = String::new();
        std::io::stdin().read_to_string(&mut code).unwrap();
        code
    } else { fs::read_to_string(&args[1]).unwrap() };
    if let Ok(path) = std::env::var("KNOT_TEST_FORMAT_LOG") {
        writeln!(fs::OpenOptions::new().create(true).append(true).open(path).unwrap(), "{}", if python { "python" } else { "r" }).unwrap();
    }
    if code.contains("FAIL") {
        eprintln!("fixture parse error");
        std::process::exit(2);
    }
    let formatted = code.replace("x=1", "x = 1").replace("x<-1", "x <- 1");
    if python { print!("{formatted}"); } else { fs::write(&args[1], formatted).unwrap(); }
}

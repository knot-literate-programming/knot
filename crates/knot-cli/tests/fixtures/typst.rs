fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args[0], "compile");
    assert_eq!(args[1], "--root");
    match std::env::var("KNOT_TEST_TYPST_MODE").unwrap().as_str() {
        "error" => {
            eprintln!("error: unknown variable: missing\n  ┌─ {}:12:3\n  │\n12│ #missing\n  │  ^^^^^^^\nhelp: check the variable name", args[3]);
            std::process::exit(2);
        }
        "silent" => std::process::exit(3),
        "warning" => {
            eprintln!("warning: deprecated syntax");
            std::fs::write(&args[4], b"%PDF-fixture").unwrap();
        }
        _ => panic!("unexpected mode"),
    }
}

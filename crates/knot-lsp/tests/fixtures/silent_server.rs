// Never read stdin, respond, or honor shutdown/exit: exercise pipe backpressure.
fn main() {
    loop { std::thread::park(); }
}

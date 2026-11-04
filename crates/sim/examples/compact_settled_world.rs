fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "input output");
    crabsim::seed_compact::compact(
        std::path::Path::new(&args[0]),
        std::path::Path::new(&args[1]),
    );
}

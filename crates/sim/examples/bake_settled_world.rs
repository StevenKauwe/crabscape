fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--write") => crabsim::underwater::seed_bake::bake(std::path::Path::new(
            args.get(1).expect("output path"),
        )),
        Some("--probe") => crabsim::underwater::seed_bake::probe(
            args.get(1)
                .map_or("30", String::as_str)
                .parse()
                .expect("probe steps"),
        ),
        _ => panic!("use --probe [steps] or --write output-file"),
    }
}

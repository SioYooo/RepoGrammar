fn main() {
    let output = repogrammar::interfaces::cli::run(std::env::args().skip(1));
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    std::process::exit(output.status);
}

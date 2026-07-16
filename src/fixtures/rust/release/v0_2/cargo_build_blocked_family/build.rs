fn main() {
    std::fs::write("build-script-ran.txt", "unexpected execution").unwrap();
}

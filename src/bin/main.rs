fn main() {
    std::process::exit(xlsx2csv_rs::cli::run(std::env::args().skip(1)));
}

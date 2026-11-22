#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = notwig_ext_cli::run() {
        eprintln!("notwig-ext-cli: {error}");
        std::process::exit(1);
    }
}

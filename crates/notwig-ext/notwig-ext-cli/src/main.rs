#![forbid(unsafe_code)]

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = notwig_ext_cli::run(&args) {
        eprintln!("notwig-ext-cli: {error}");
        std::process::exit(1);
    }
}

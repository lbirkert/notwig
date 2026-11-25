#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = cargo_notwig_ext::run() {
        eprintln!("cargo notwig-ext: {error}");
        std::process::exit(1);
    }
}

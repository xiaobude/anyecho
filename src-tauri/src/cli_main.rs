use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        anyecho_lib::cli::run_cli(&[]);
    } else {
        anyecho_lib::cli::run_cli(&args[1..]);
    }
}


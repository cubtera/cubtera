#![warn(clippy::all, clippy::pedantic)]
#![allow(dead_code, unused_variables)]

mod cmd;

fn main() {
    // Logger init
    cubtera::utils::logger_init();

    // CLI
    if let Err(e) = cmd::get_args().and_then(cmd::run) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

use gravstar_game::app::{self, Options};

fn main() {
    let options = match Options::from_args(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    if let Err(message) = app::run(&options) {
        eprintln!("GravStar: {message}");
        std::process::exit(1);
    }
}

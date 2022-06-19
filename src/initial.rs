use clap::Parser;
use crate::wasm::Wasm;
use crate::route_config::Config;

/// Load and run a Wasm as a Haiku Connector
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path of the route config
    #[clap(short, long, value_parser)]
    config: String,

    /// Path of the Wasm file
    #[clap(short, long, value_parser)]
    wasm: String,
}

pub struct Initial {
    pub wasm: Wasm,
    pub config: Config,
}

impl Initial {
    pub fn new() -> Initial {
        let args = Args::parse();
        Initial {
            wasm: Wasm::new(args.wasm),
            config: Config::new(args.config),
        }
    }
}
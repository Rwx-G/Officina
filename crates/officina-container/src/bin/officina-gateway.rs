//! `officina-gateway <args>` (AD-103): the egress gateway of one run. Its
//! command line is its whole configuration (core's `GatewayConfig`); a
//! command line that does not parse exits 64.

use officina_core::workshop::gateway::GatewayConfig;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match GatewayConfig::from_args(&args) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("officina-gateway: {e}");
            std::process::exit(64);
        }
    };
    std::process::exit(officina_container::gateway::serve(
        config,
        Box::new(std::io::stdout()),
    ));
}

fn main() {
    let cli: Vec<String> = std::env::args().skip(1).collect();
    if let Some(request) = voxl::autostart::request(&cli) {
        if let Err(e) = voxl::autostart::handle(request) {
            eprintln!("voxl: {e}");
            std::process::exit(1);
        }
        return;
    }
    let mut args = voxl::config::config_args().unwrap_or_else(|e| {
        eprintln!("voxl: {e}");
        std::process::exit(1);
    });
    if !args.is_empty() {
        eprintln!(
            "voxl: flags from {} and the command line",
            voxl::config::config_path().display()
        );
    }
    args.extend(cli);
    voxl::run(args);
}

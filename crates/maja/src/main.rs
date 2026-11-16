fn main() {
    std::process::exit(maja::run(
        std::env::args_os(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    ));
}

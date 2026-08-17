fn main() {
    if std::env::args().any(|argument| argument == "--hawk-mcp-stdio") {
        if let Err(error) = hawk_code_desktop_lib::run_builtin_mcp_stdio() {
            eprintln!("{error}");
            std::process::exit(1);
        }
    } else {
        hawk_code_desktop_lib::run();
    }
}

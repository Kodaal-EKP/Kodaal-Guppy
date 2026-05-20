use std::io::Write;

#[tokio::main]
async fn main() {
    let code = kodaal_core::cli::run().await;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code);
}

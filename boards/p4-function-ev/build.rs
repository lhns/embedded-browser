fn main() {
    // Emit the ESP-IDF linker arguments (no-op for host builds).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        embuild::espidf::sysenv::output();
    }
}

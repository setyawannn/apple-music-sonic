fn main() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("[FATAL CRASH / PANIC] {}", info);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("crash.log") {
            use std::io::Write;
            let _ = writeln!(f, "[FATAL CRASH / PANIC] {}", info);
        }
    }));
    println!("[MAIN] Starting application...");
    app_lib::run();
    println!("[MAIN] app_lib::run() returned normally.");
}

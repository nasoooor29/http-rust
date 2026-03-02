/// Simple logger macro with colored structured output
#[macro_export]
macro_rules! log {
    ($level:expr, $msg:expr $(, $key:expr => $val:expr )* ) => {{
        // ANSI color codes
        let color = match $level {
            "INFO" => "\x1b[32m",   // Green
            "WARN" => "\x1b[33m",   // Yellow
            "ERROR" => "\x1b[31m",  // Red
            "DEBUG" => "\x1b[34m",  // Blue
            _ => "\x1b[0m",          // Reset
        };
        let reset = "\x1b[0m";
        let mut out = String::new();
        out.push_str(&format!("{}[{}]{} {}", color, $level, reset, $msg));
        $( out.push_str(&format!(" | {}: {}", $key, $val)); )*
        // Print with line number and filename
        let line = line!();
        let file = file!();
        println!("[{}:{}] {}", file, line, out);
    }};
}

#[macro_export]
macro_rules! info {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        log!("INFO", $msg $(, $key => $val )* );
    }}
}

#[macro_export]
macro_rules! warn {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        log!("WARN", $msg $(, $key => $val )* );
    }}
}

#[macro_export]
macro_rules! debug {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        log!("DEBUG", $msg $(, $key => $val )* );
    }}
}

#[macro_export]
macro_rules! error {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        log!("ERROR", $msg $(, $key => $val )* );
    }}
}

// Example usage:
// log!("INFO", "User logged in", "user_id" => 42, "ip" => "127.0.0.1");

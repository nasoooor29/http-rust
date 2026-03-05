#[macro_export]
macro_rules! info {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        let mut out = String::new();
        out.push_str(&format!("\x1b[32m[INFO]\x1b[0m {}", $msg));
        $( out.push_str(&format!(" | {}: {}", $key, $val)); )*
        println!("[{}:{}] {}", file!(), line!(), out);
    }}
}

#[macro_export]
macro_rules! warn {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        let mut out = String::new();
        out.push_str(&format!("\x1b[33m[WARN]\x1b[0m {}", $msg));
        $( out.push_str(&format!(" | {}: {}", $key, $val)); )*
        eprintln!("[{}:{}] {}", file!(), line!(), out);
    }}
}

#[macro_export]
macro_rules! debug {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        let mut out = String::new();
        out.push_str(&format!("\x1b[34m[DEBUG]\x1b[0m {}", $msg));
        $( out.push_str(&format!(" | {}: {}", $key, $val)); )*
        println!("[{}:{}] {}", file!(), line!(), out);
    }}
}

#[macro_export]
macro_rules! error {
    ($msg:expr $(, $key:expr => $val:expr )* ) => {{
        let mut out = String::new();
        out.push_str(&format!("\x1b[31m[ERROR]\x1b[0m {}", $msg));
        $( out.push_str(&format!(" | {}: {}", $key, $val)); )*
        eprintln!("[{}:{}] {}", file!(), line!(), out);
    }}
}

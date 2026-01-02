use std::process::exit;
use std::sync::Mutex;
use std::time::SystemTime;
use lazy_static::lazy_static;

struct LogContext {
    verbose: bool
}



lazy_static!(
    static ref context: Mutex<LogContext> = Mutex::new(LogContext{ verbose: false });
);


pub fn enable_verbose_logging() {
    context.lock().unwrap().verbose = true;
}


pub fn log(msg: &str, verbose: bool) {
    if verbose && !context.lock().unwrap().verbose {
        return;
    }

    let timestamp = chrono::Local::now();

    println!("[FLUX][{}]{}: {}",
             timestamp.format("%H:%M"),
        if verbose { "[VERBOSE]" } else { "" },
        msg
    );
}

pub fn err_log(msg: &str, verbose: bool) {
    if verbose && !context.lock().unwrap().verbose {
        return;
    }

    let timestamp = chrono::Local::now();

    eprintln!("[FLUX][ERROR][{}]{}: {}",
             timestamp.format("%H:%M"),
             if verbose { "[VERBOSE]" } else { "" },
             msg
    );
}


pub fn fatal_log(msg: &str) -> ! {

    let timestamp = chrono::Local::now();

    eprintln!("[FLUX][FATAL][{}]: {}",
              timestamp.format("%H:%M"),
              msg
    );

    exit(-1);
}

#[macro_export]
macro_rules! log {
    ($verbose: expr, $($arg:tt)*) => {
        log(format!($($arg)*).as_str(), $verbose)
    };
}

#[macro_export]
macro_rules! err_log {
    ($verbose: expr, $($arg:tt)*) => {
        err_log(format!($($arg)*).as_str(), $verbose)
    };
}

#[macro_export]
macro_rules! fatal_log {
    ($($arg:tt)*) => {
        fatal_log(format!($($arg)*).as_str())
    };
}
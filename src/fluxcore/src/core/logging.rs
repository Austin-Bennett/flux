use std::io;
use std::io::{Stdout, Write};
use std::process::exit;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use lazy_static::lazy_static;
use crossterm::{cursor, QueueableCommand};
use crossterm::terminal::{Clear, ClearType};
use crate::utils::Ignore;

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


    println!("{}{}",
        if verbose { "[VERBOSE]: " } else { "" },
        msg
    );
}

pub fn err_log(msg: &str, verbose: bool) {
    if verbose && !context.lock().unwrap().verbose {
        return;
    }

    let timestamp = chrono::Local::now();

    eprintln!("[ERROR][{}]{}: {}",
            timestamp.format("%H:%M"),
             if verbose { "[VERBOSE]" } else { "" },
             msg
    );
}


pub fn fatal_log(msg: &str) -> ! {

    let timestamp = chrono::Local::now();

    eprintln!("[FATAL][{}]: {}",
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


//draws a progress bar, when you print a line, it moves the progress bar down 1
pub struct ProgressLogger {
    max: u64,
    cur: u64,
    row: u16,
    stdout: Stdout,
}

const PB_WIDTH: u64 = 30;

impl ProgressLogger {
    pub fn new(max: u64) -> io::Result<Self> {
        let stdout = io::stdout();
        let (_, row) = cursor::position()?;
        let mut res = ProgressLogger{ cur: 0, max, row, stdout};

        res.draw_progress_bar()?;

        Ok(res)
    }

    fn draw_progress_bar(&mut self) -> io::Result<()> {

        self.stdout
            .queue(cursor::MoveTo(0, self.row))?
            .queue(Clear(ClearType::CurrentLine))?;

        let mut pb = String::new();
        pb.reserve((PB_WIDTH+10) as usize);

        let percent = self.cur as f64 / self.max as f64;
        let filled = (percent * PB_WIDTH as f64) as u64;

        pb.push('[');
        for i in 0..PB_WIDTH {
            if i <= filled {
                pb.push('=');
            }else if i == filled + 1 {
                pb.push('>');
            } else {
                pb.push(' ');
            }
        }
        pb.push(']');


        write!(self.stdout, "{} {}/{} ({:.1}%)", pb, self.cur, self.max, percent * 100.0)?;

        self.stdout.flush()?;

        Ok(())
    }

    pub fn println(&mut self, str: impl AsRef<str>) -> io::Result<()> {
        self.stdout
            .queue(cursor::MoveTo(0, self.row))?
            .queue(Clear(ClearType::CurrentLine))?;


        self.stdout.write(str.as_ref().as_bytes())?;

        self.stdout.flush()?;

        self.row += 1;
        self.draw_progress_bar()?;

        Ok(())
    }

    pub fn set_progress(&mut self, prog: u64) {
        self.cur = prog.min(self.max);
        self.draw_progress_bar().ignore();
    }

    pub fn inc(&mut self, n: u64) {
        self.cur = (self.cur + n).min(self.max);
        self.draw_progress_bar().ignore();
    }

    pub fn finalize(&mut self) -> io::Result<()> {
        let _ = writeln!(self.stdout);

        Ok(())
    }
}


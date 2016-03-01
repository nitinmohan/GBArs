// License below.

use std::io::Write;
use std::fs::File;
use std::path::Path;
use std::sync::Mutex;
use std::cell::RefCell;
use std::thread;
use log::{set_logger, Log, LogMetadata, LogRecord, LogLevel, LogLevelFilter, SetLoggerError};


pub struct ConsoleFileLogger {
    pub file: Option<Mutex<RefCell<File>>>,
    pub verbose: bool,
    pub colour: bool,
}

impl Log for ConsoleFileLogger {
    fn enabled(&self, metadata: &LogMetadata) -> bool {
        let min_level = if self.verbose { LogLevel::Trace } else { LogLevel::Info };
        metadata.level() <= min_level
    }

    fn log(&self, record: &LogRecord) {
        if self.enabled(record.metadata()) {
            // Prepare some common message sections in case of colouring.
            let cur = thread::current();
            let tid = cur.name().unwrap_or("<?>");
            let loc = record.location();
            let loc = format!("[{}:{} - {}]", loc.file(), loc.line(), loc.module_path());
            let fmt = format!("{}", record.args()).replace("\n","\n\t\t   ");
            
            // Build a common log message for both targets.
            let msg = format!("[TID={}]\t{}\t{}\n\t\t-- {}\n", tid, record.level(), loc, fmt);
            
            // Log to file.
            if let Some(f) = self.file.as_ref() {
                let tmp = f.lock().unwrap();
                writeln!(*(tmp.borrow_mut()), "{}", msg).unwrap();
            }
            
            // Log to stdout.
            if !self.colour { println!("{}", msg); }
            else {
                // Colourising is only done for terminals.
                println!(
                    "\x1B[0m\x1B[2m[TID={}]\t{}{}\x1B[0m\x1B[2m\t{}\x1B[1m\n\t\t-- {}\x1B[0m\n",
                    tid, match record.level() {
                        LogLevel::Error => "\x1B[31m\x1B[1m", // Bold, red.
                        LogLevel::Warn  => "\x1B[33m\x1B[1m", // Bold, yellow.
                        LogLevel::Info  => "\x1B[32m\x1B[1m", // Bold, green.
                        _               => "\x1B[34m\x1B[1m", // Bold, blue.
                    }, record.level(), loc, fmt
                );
            }
        }
    }
}


pub fn init_with(file: &Path, verbose: bool, colour: bool) -> Result<(), SetLoggerError> {
    set_logger(|max_log_level| {
        max_log_level.set(LogLevelFilter::Trace);
        box ConsoleFileLogger {
            file: Some(Mutex::new(RefCell::new(File::create(file).unwrap()))),
            verbose: verbose,
            colour: colour,
        }
    })
}


/*
Licensed to the Apache Software Foundation (ASF) under one
or more contributor license agreements.  See the NOTICE file
distributed with this work for additional information
regarding copyright ownership.  The ASF licenses this file
to you under the Apache License, Version 2.0 (the
"License"); you may not use this file except in compliance
with the License.  You may obtain a copy of the License at

  http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing,
software distributed under the License is distributed on an
"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
KIND, either express or implied.  See the License for the
specific language governing permissions and limitations
under the License.
*/

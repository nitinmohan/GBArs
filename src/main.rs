

#![feature(box_syntax)]

#[macro_use]
extern crate log;
extern crate argparse;

use argparse::{ArgumentParser, Print, Parse, ParseOption, StoreTrue};
use std::path::PathBuf;

mod logger;
mod hardware;


struct CmdLineArgs {
    //
    rom_file_path: Option<PathBuf>,
    
    //
    log_file_path: PathBuf,
    
    //
    verbose: bool,
}

impl Default for CmdLineArgs {
    //
    fn default() -> CmdLineArgs {
        CmdLineArgs {
            rom_file_path: None,
            log_file_path: PathBuf::from("./rsGBA.log"),
            verbose: false,
            pretty: true,
        }
    }
}


fn main() {
    // Build command line parser.
    let mut args = CmdLineArgs::default();
    {
        let mut parser = ArgumentParser::new();
        parser.set_description("A GBA emulator written in Rust.");
        parser.add_option(&["-V", "--version"],
                          Print(format!("rsGBA v{}", env!("CARGO_PKG_VERSION"))),
                          "Show current version.");
        parser.refer(&mut args.rom_file_path)
              .add_option(&["--rom"], ParseOption, "Path to a ROM file to load.")
              .metavar("PATH");
        parser.refer(&mut args.log_file_path)
              .add_option(&["--log"], Parse, "Custom path for the log file.")
              .metavar("PATH");
        parser.refer(&mut args.verbose)
              .add_option(&["-v","--verbose"], StoreTrue, "Log extra messages and information.");
        parser.parse_args_or_exit();
    }
    
    // Handle command line arguments.
    {
        let p = args.log_file_path.as_path();
        logger::init_with(&p, args.verbose).unwrap();
        info!("Logging to file `{}`.\nTest.", p.to_str().unwrap_or("<?>"));
    }
}

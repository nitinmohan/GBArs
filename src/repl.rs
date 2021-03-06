// License below.
//! Provides a debug REPL for the GBA emulator.
#![cfg_attr(feature="clippy", warn(result_unwrap_used, option_unwrap_used, print_stdout))]
#![cfg_attr(feature="clippy", warn(single_match_else, string_add, string_add_assign))]
#![cfg_attr(feature="clippy", warn(wrong_pub_self_convention))]
#![warn(missing_docs)]

use super::hardware;
use super::term_painter::ToStyle;
use super::term_painter::Color::*;
use super::term_painter::Attr::Plain;
use std::u32;
use std::io;
use std::io::Write;
use std::str::SplitWhitespace;

/// Implements a debug REPL for the GBA emulator.
///
/// REPL stands for **R**ead, **E**val, **P**rint, **L**oop,
/// so all it does is running the emulator step by step waiting
/// for user inputs.
pub struct GbaRepl {
    diff_arm7tdmi: hardware::cpu::Arm7TdmiDiff,
    colour: bool,
    show_arm7tdmi: bool,
}

impl GbaRepl {
    /// Creates a new REPL without running it.
    pub fn new() -> GbaRepl {
        GbaRepl {
            diff_arm7tdmi: hardware::cpu::Arm7TdmiDiff::new(),
            colour: true,
            show_arm7tdmi: true,
        }
    }

    /// Configure whether terminal outputs should be colourised.
    pub fn with_colour(&mut self, c: bool) -> &mut GbaRepl { self.colour = c; self }

    /// Runs the REPL until the user quits, an error occurred,
    /// or until the emulated program ends.
    pub fn run(&mut self, gba: &mut hardware::Gba) -> Result<(), hardware::GbaError> {
        // Prepare everything we need.
        gba.cpu_arm7tdmi_mut().reset();
        self.diff_arm7tdmi.diff(gba.cpu_arm7tdmi());
        self.print_emu();
        let mut input = String::new();

        // Now run the actual REPL.
        loop {
            if let Err(e) = self.input_prompt(&mut input) { error!("{}", e); break; }
            let mut s = input.trim().split_whitespace();

            match s.next() {
                Some("?") => self.print_help(),
                Some("x") => break,
                Some("p") => self.print_emu(),
                Some("hex") => if let Some(r) = s.next() { GbaRepl::hexdump(r, gba); },
                Some("run") => if let Some(n) = s.next() { try!(self.run_n_steps_str(gba, n)); },
                Some("toggle") => if let Some(cpu) = s.next() { self.toggle_cpu(cpu); },
                Some("") | None => try!(self.run_n_steps(gba, 1)),
                _ => print!("\t\t<What?>\n\n"),
            }
        }
        Ok(())
    }

    fn input_prompt(&self, input: &mut String) -> io::Result<()> {
        print!("\t{}\n\t> ", Black.bg(White).paint("[? = Help, x = Exit, p, hex A..B, run N, toggle CPU]"));
        io::stdout().flush().unwrap();
        input.clear();
        try!(io::stdin().read_line(input));
        println!("");
        Ok(())
    }

    fn print_help(&self) {
        println!("\t{}\n\t\
            ?          - Print this help text.\n\t\
            x          - Exit the debug REPL.\n\t\
            p          - Print the current CPU state again.\n\t\
            hex RANGE  - Hexdump a region of memory defined by RANGE.\n\t\
            run N      - Run N pipeline steps, where N is a positive integer.\n\t\
            toggle CPU - Show/hide the current state of CPU.\n\t\
            [ENTER]    - Just hit the enter key to run a single pipeline step.\n\t\
            \n\t{}\n\t\
            RANGE - A pair of baseless hexadecimal values, e.g. `A..B`.\n\t        \
                    The default range is `0..80` and any omitted value\n\t        \
                    will be interpreted as the default value. Thus, `..B`\n\t        \
                    will be interpreted as `0..B`.\n\t\
            CPU   - A CPU name. The possible values are:\n\t        \
                    - all\n\t        \
                    - Arm7Tdmi\n\t",
            BrightWhite.paint("Commands:"), BrightWhite.paint("Arguments:"),
        );
    }

    fn print_emu(&self) {
        if self.show_arm7tdmi { self.diff_arm7tdmi.print(); }
    }

    fn emu_step(&self, gba: &mut hardware::Gba) -> Result<(), hardware::GbaError> {
        gba.cpu_arm7tdmi_mut().pipeline_step()
    }

    fn diff(&mut self, gba: &hardware::Gba) {
        self.diff_arm7tdmi.diff(gba.cpu_arm7tdmi());
    }

    fn run_n_steps(&mut self, gba: &mut hardware::Gba, n: u32) -> Result<(), hardware::GbaError> {
        for _ in 0..n { try!(self.emu_step(gba)); }
        self.diff(gba);
        self.print_emu();
        Ok(())
    }

    fn run_n_steps_str(&mut self, gba: &mut hardware::Gba, n: &str) -> Result<(), hardware::GbaError> {
        match u32::from_str_radix(n, 10) {
            Ok(n)  => self.run_n_steps(gba, n),
            Err(e) => { error!("{}", e); Ok(()) },
        }
    }

    fn hexdump(s: &str, gba: &hardware::Gba) {
        if let Some(mut r) = super::parse_hex_range(&s, 0x00, 0x80) {
            r.start &= !31;
            r.end   +=  31;
            r.end   &= !31;
            print!("\t\t           00           04           08           0C           \
                                   10           14           18           1C");
            for i in r {
                if (i % 32) == 0 { print!("\n\t\t{:08X} -", i); }
                else if (i % 4) == 0 { print!(" "); }
                print!(" {:02X}", gba.bus().load_byte(i).unwrap_or(0));
            }
            print!("\n\n");
        }
    }

    fn toggle_cpu(&mut self, cpu: &str) {
        match cpu {
            "Arm7Tdmi" => { self.show_arm7tdmi = !self.show_arm7tdmi; },
            "all" => {
                self.show_arm7tdmi = !self.show_arm7tdmi;
            },
            _ => {},
        }
    }
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

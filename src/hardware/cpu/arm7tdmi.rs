// License below.
//! Implements emulation utilities for the GBA's main CPU, the ARM7TDMI.
#![cfg_attr(feature="clippy", warn(result_unwrap_used, option_unwrap_used, print_stdout))]
#![cfg_attr(feature="clippy", warn(single_match_else, string_add, string_add_assign))]
#![cfg_attr(feature="clippy", warn(wrong_pub_self_convention))]
#![warn(missing_docs)]

use std::mem;
use std::u32;
use super::arminstruction::{ArmInstruction, ArmOpcode, ArmDPOP};
use super::super::error::GbaError;


/// The CPU's instruction decoding states.
#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum State {
    /// Currently executing 32-bit ARM instructions.
    ARM = 0,

    /// Currently executing 16-bit THUMB instructions.
    THUMB,
}

/// The CPU's different execution modes.
#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum Mode {
    #[doc = "CPU mode for running normal user code."]                  User = 0,
    #[doc = "CPU mode for handling fast interrupts."]                  FIQ,
    #[doc = "CPU mode for handling normal interrupts."]                IRQ,
    #[doc = "CPU mode for executing supervisor code."]                 Supervisor,
    #[doc = "CPU mode entered if memory lookups are aborted."]         Abort,
    #[doc = "CPU mode entered if executing an undefined instruction."] Undefined,
    #[doc = "CPU mode for executing system code."]                     System,
}

impl Mode {
    /// Converts this mode into a CPSR bit pattern.
    pub fn as_bits(self) -> u32 {
        match self {
            Mode::User       => CPSR::MODE_USER,
            Mode::FIQ        => CPSR::MODE_FIQ,
            Mode::IRQ        => CPSR::MODE_IRQ,
            Mode::Supervisor => CPSR::MODE_SUPERVISOR,
            Mode::Abort      => CPSR::MODE_ABORT,
            Mode::Undefined  => CPSR::MODE_UNDEFINED,
            Mode::System     => CPSR::MODE_SYSTEM
        }
    }
}


/// CPU exceptions.
#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum Exception {
    #[doc = "Exception due to resetting the CPU."]                Reset,
    #[doc = "Exception due to executing undefined instructions."] UndefinedInstruction,
    #[doc = "Exception due to executing SWI."]                    SoftwareInterrupt,
    #[doc = "Instruction prefetching aborted."]                   PrefetchAbort,
    #[doc = "Data prefetching aborted."]                          DataAbort,
    #[doc = "Exception due to resolving large addresses."]        AddressExceeds26Bit,
    #[doc = "Exception due to a normal hardware interrupt."]      NormalInterrupt,
    #[doc = "Exception due to a fast hardware interrupt."]        FastInterrupt,
}

impl Exception {
    /// Get the exception's priority.
    ///
    /// # Returns
    /// 1 = highest, 7 = lowest.
    pub fn priority(self) -> u8 {
        match self {
            Exception::Reset                => 1,
            Exception::UndefinedInstruction => 7,
            Exception::SoftwareInterrupt    => 6,
            Exception::PrefetchAbort        => 5,
            Exception::DataAbort            => 2,
            Exception::AddressExceeds26Bit  => 3,
            Exception::NormalInterrupt      => 4,
            Exception::FastInterrupt        => 3,
        }
    }

    /// Get the exception's CPU mode on entry.
    pub fn mode_on_entry(self) -> Mode {
        match self {
            Exception::Reset                => Mode::Supervisor,
            Exception::UndefinedInstruction => Mode::Undefined,
            Exception::SoftwareInterrupt    => Mode::Supervisor,
            Exception::PrefetchAbort        => Mode::Abort,
            Exception::DataAbort            => Mode::Abort,
            Exception::AddressExceeds26Bit  => Mode::Supervisor,
            Exception::NormalInterrupt      => Mode::IRQ,
            Exception::FastInterrupt        => Mode::FIQ,
        }
    }

    /// Check whether fast interrupts should be disabled.
    ///
    /// # Returns
    /// - `true` if FIQ should be disabled on entry.
    /// - `false` if FIQ should be left unchanged.
    #[inline(always)]
    pub fn disable_fiq_on_entry(self) -> bool {
        (self == Exception::Reset) | (self == Exception::FastInterrupt)
    }

    /// Get the exception vector address.
    ///
    /// # Returns
    /// A physical address to the exception's
    /// vector entry.
    #[inline(always)]
    pub fn vector_address(self) -> u32 {
        (self as u8 as u32) * 4
    }
}


/// The Current Program Status Register.
#[derive(PartialEq, Clone, Copy)]
pub struct CPSR(u32);

impl CPSR {
    /// Used to mask reserved bits away.
    pub const NON_RESERVED_MASK: u32 = 0b11110000_00000000_00000000_11111111_u32;
    //                                   NZCV                       IFTMMMMM

    /// Sign flag bit.
    ///
    /// 1 if signed, otherwise 0.
    pub const SIGN_FLAG_BIT: u8 = 31;

    /// Zero flag bit.
    ///
    /// 1 if zero, otherwise 0.
    pub const ZERO_FLAG_BIT: u8 = 30;

    /// Carry flag bit.
    ///
    /// 1 if carry or no borrow, 0 if borrow or no carry.
    pub const CARRY_FLAG_BIT: u8 = 29;

    /// Overflow flag bit.
    ///
    /// 1 if overflow, otherwise 0.
    pub const OVERFLOW_FLAG_BIT: u8 = 28;

    /// IRQ disable bit.
    ///
    /// 1 if disabled, otherwise 0.
    pub const IRQ_DISABLE_BIT: u8 = 7;

    /// FIQ disable bit.
    ///
    /// 1 if disabled, otherwise 0.
    pub const FIQ_DISABLE_BIT: u8 = 6;

    /// State bit.
    ///
    /// 1 if THUMB, 0 if ARM.
    pub const STATE_BIT: u8 = 5;

    /// Mode bits mask.
    ///
    /// Used to get the mode bits only.
    pub const MODE_MASK: u32 = 0b0001_1111;

    /// Bit pattern for user mode.
    pub const MODE_USER: u32 = 0b1_0000;

    /// Bit pattern for FIQ mode.
    pub const MODE_FIQ: u32 = 0b1_0001;

    /// Bit pattern for IRQ mode.
    pub const MODE_IRQ: u32 = 0b1_0010;

    /// Bit pattern for supervisor mode.
    pub const MODE_SUPERVISOR: u32 = 0b1_0011;

    /// Bit pattern for abort mode.
    pub const MODE_ABORT: u32 = 0b1_0111;

    /// Bit pattern for undefined mode.
    pub const MODE_UNDEFINED: u32 = 0b1_1011;

    /// Bit pattern for system mode.
    pub const MODE_SYSTEM: u32 = 0b1_1111;


    /// Clears all reserved bits.
    #[inline(always)]
    pub fn clear_reserved_bits(&mut self) {
        self.0 &= CPSR::NON_RESERVED_MASK;
    }

    /// Get the condition bits.
    ///
    /// # Returns
    /// The condition bits are laid out as such:
    /// ```
    /// 0b0000
    /// //NZCV
    /// ```
    #[inline(always)]
    pub fn condition_bits(&self) -> u32 {
        (self.0 as u32) >> CPSR::OVERFLOW_FLAG_BIT
    }

    /// Converts the state bit to a state enum.
    #[inline(always)]
    pub fn state(&self) -> State {
        unsafe { mem::transmute(((self.0 >> CPSR::STATE_BIT) & 1) as u8) }
    }

    /// Converts the mode bit pattern to a mode enum.
    pub fn mode(&self) -> Mode {
        match self.0 & CPSR::MODE_MASK {
            CPSR::MODE_USER       => Mode::User,
            CPSR::MODE_FIQ        => Mode::FIQ,
            CPSR::MODE_IRQ        => Mode::IRQ,
            CPSR::MODE_SUPERVISOR => Mode::Supervisor,
            CPSR::MODE_ABORT      => Mode::Abort,
            CPSR::MODE_UNDEFINED  => Mode::Undefined,
            CPSR::MODE_SYSTEM     => Mode::System,
            _ => {
                error!("CPSR: Unrecognised mode bit pattern {:#8b}.", self.0 & CPSR::MODE_MASK);
                panic!("Aborting due to illegal mode bits.");
            },
        }
    }

    /// Sets or clears the state bit
    /// depending on the new state.
    #[inline(always)]
    pub fn set_state(&mut self, s: State) {
        self.0 &= !(1 << CPSR::STATE_BIT);
        self.0 |= (s as u8 as u32) << CPSR::STATE_BIT;
    }

    /// Sets or clears the mode bits
    /// depending on the new mode.
    #[inline(always)]
    pub fn set_mode(&mut self, m: Mode) {
        self.0 &= !CPSR::MODE_MASK;
        self.0 |= m.as_bits();
    }

    /// Sets the IRQ disable bit.
    #[inline(always)]
    pub fn disable_irq(&mut self) {
        self.0 |= 1 << CPSR::IRQ_DISABLE_BIT;
    }

    /// Sets the FIQ disable bit.
    #[inline(always)]
    pub fn disable_fiq(&mut self) {
        self.0 |= 1 << CPSR::FIQ_DISABLE_BIT;
    }

    /// Clears the IRQ disable bit.
    #[inline(always)]
    pub fn enable_irq(&mut self) {
        self.0 &= !(1 << CPSR::IRQ_DISABLE_BIT);
    }

    /// Clears the FIQ disable bit.
    #[inline(always)]
    pub fn enable_fiq(&mut self) {
        self.0 &= !(1 << CPSR::FIQ_DISABLE_BIT);
    }

    /// Gets the current state of the N bit.
    #[allow(non_snake_case)]
    pub fn N(self) -> bool { 0 != (self.0 & (1 << 31)) }

    /// Gets the current state of the Z bit.
    #[allow(non_snake_case)]
    pub fn Z(self) -> bool { 0 != (self.0 & (1 << 30)) }

    /// Gets the current state of the C bit.
    #[allow(non_snake_case)]
    pub fn C(self) -> bool { 0 != (self.0 & (1 << 29)) }

    /// Gets the current state of the V bit.
    #[allow(non_snake_case)]
    pub fn V(self) -> bool { 0 != (self.0 & (1 << 28)) }

    /// Set the new state of the N bit.
    #[allow(non_snake_case)]
    pub fn set_N(&mut self, n: bool) { if n { self.0 |= 1 << 31; } else { self.0 &= !(1 << 31); } }

    /// Set the new state of the Z bit.
    #[allow(non_snake_case)]
    pub fn set_Z(&mut self, n: bool) { if n { self.0 |= 1 << 30; } else { self.0 &= !(1 << 30); } }

    /// Set the new state of the C bit.
    #[allow(non_snake_case)]
    pub fn set_C(&mut self, n: bool) { if n { self.0 |= 1 << 29; } else { self.0 &= !(1 << 29); } }

    /// Set the new state of the V bit.
    #[allow(non_snake_case)]
    pub fn set_V(&mut self, n: bool) { if n { self.0 |= 1 << 28; } else { self.0 &= !(1 << 28); } }
}


/// TODO
pub struct Arm7Tdmi {
    // Main register set.
    gpr: [i32; 16],
    cpsr: CPSR,
    spsr: [u32; 7],

    // Pipeline implementation.
    decoded: ArmInstruction,
    fetched: u32,

    // Register backups for mode changes.
    gpr_r8_r12_fiq: [i32; 5],
    gpr_r8_r12_other: [i32; 5],
    gpr_r13_all: [i32; 7],
    gpr_r14_all: [i32; 7],

    // Settings.
    mode: Mode,
    state: State,
    irq_disable: bool,
    fiq_disable: bool,
}

impl Arm7Tdmi {
    /// Register index for the stack pointer.
    ///
    /// May be used as GPR in ARM state.
    pub const SP: usize = 13;

    /// Register index for the link register.
    ///
    /// This register usually holds the returns address
    /// of a running function. In ARM state, this might
    /// be used as GPR.
    pub const LR: usize = 14;

    /// Register index for the program counter.
    ///
    /// When reading PC, this will usually return an
    /// address beyond the read instruction's address,
    /// due to pipelining and other things.
    pub const PC: usize = 15;

    /// Creates a new CPU where all registers are zeroed.
    pub fn new() -> Arm7Tdmi {
        Arm7Tdmi {
            gpr: [0; 16],
            cpsr: CPSR(0),
            spsr: [0; 7],

            decoded: ArmInstruction::nop(),
            fetched: ArmInstruction::NOP_RAW,

            gpr_r8_r12_fiq: [0; 5],
            gpr_r8_r12_other: [0; 5],
            gpr_r13_all: [0; 7],
            gpr_r14_all: [0; 7],

            mode: Mode::System,
            state: State::ARM,
            irq_disable: false,
            fiq_disable: false,
        }
    }

    /// Resets the CPU.
    ///
    /// The CPU starts up by setting few
    /// register states and entering a
    /// reset exception.
    pub fn reset(&mut self) {
        self.gpr[Arm7Tdmi::PC] = 0;

        self.cpsr = CPSR(
            (CPSR::MODE_SUPERVISOR)
          | (1 << CPSR::IRQ_DISABLE_BIT)
          | (1 << CPSR::FIQ_DISABLE_BIT)
        );

        self.mode = Mode::Supervisor;
        self.state = State::ARM;
        self.irq_disable = true;
        self.fiq_disable = true;
    }

    /// Causes an exception, switching execution modes and states.
    pub fn exception(&mut self, ex: Exception) {
        self.change_mode(ex.mode_on_entry());
        self.cpsr.set_state(State::ARM);
        self.state = State::ARM;
        self.cpsr.disable_irq();
        if ex.disable_fiq_on_entry() { self.cpsr.disable_fiq(); }
        // TODO LR = PC + whatevs
        self.gpr[Arm7Tdmi::PC] = ex.vector_address() as i32;
    }


    fn change_mode(&mut self, new_mode: Mode) {
        let cmi = self.mode as u8 as usize;
        let nmi =  new_mode as u8 as usize;

        // Save banked registers R13, R14, SPSR.
        let ret_addr = self.gpr[Arm7Tdmi::PC] + 0; // TODO special offset by exception type
        self.gpr_r14_all[cmi] = self.gpr[14];
        self.gpr_r14_all[nmi] = ret_addr;
        self.gpr[14] = ret_addr;
        self.gpr_r13_all[cmi] = self.gpr[13];
        self.gpr[13] = self.gpr_r13_all[nmi];
        self.spsr[nmi] = self.cpsr.0;

        // Now the banked registers R8..R12.
        if (new_mode == Mode::FIQ) ^ (self.mode == Mode::FIQ) {
            if new_mode == Mode::FIQ {
                for i in 0..5 { self.gpr_r8_r12_other[i] = self.gpr[i+8]; }
                for i in 0..5 { self.gpr[i+8] = self.gpr_r8_r12_fiq[i]; }
            }
            else {
                for i in 0..5 { self.gpr_r8_r12_fiq[i] = self.gpr[i+8]; }
                for i in 0..5 { self.gpr[i+8] = self.gpr_r8_r12_other[i]; }
            }
        }

        // Apply new state.
        self.cpsr.set_mode(new_mode);
        self.mode = new_mode;
    }

    fn clear_pipeline(&mut self) {
        self.decoded = ArmInstruction::nop();
        self.fetched = ArmInstruction::NOP_RAW;
    }

    fn update_flags(&mut self, x: i32, y: u64) {
        self.cpsr.set_N(x  < 0);
        self.cpsr.set_Z(x == 0);
        self.cpsr.set_C((y & 0x1_00000000_u64) != 0);
        self.cpsr.set_V( y > (u32::MAX as u64));
    }

    #[allow(dead_code)] // TODO delete this
    fn execute_arm_state(&mut self, inst: ArmInstruction) -> Result<(), GbaError> {
        let do_exec = try!(inst.condition().check(&self.cpsr));
        if !do_exec { return Ok(()); }

        match inst.opcode() {
            ArmOpcode::BX             => self.execute_bx(inst),
            ArmOpcode::B_BL           => self.execute_b_bl(inst),
            ArmOpcode::MUL_MLA        => self.execute_mul_mla(inst),
            ArmOpcode::MULL_MLAL      => self.execute_mull_mlal(inst),
            ArmOpcode::DataProcessing => self.execute_data_processing(inst),
            _ => unimplemented!(),
        };

        Ok(())
    }

    fn execute_bx(&mut self, inst: ArmInstruction) {
        self.clear_pipeline();
        let addr = self.gpr[inst.Rm()] as u32;
        self.state = if (addr & 0b1) == 0 { State::ARM } else { State::THUMB };
        self.cpsr.set_state(self.state);
        self.gpr[15] = (addr & 0xFFFFFFFE) as i32;
        // TODO missaligned PC in ARM state?
    }

    fn execute_b_bl(&mut self, inst: ArmInstruction) {
        self.clear_pipeline();
        if inst.is_branch_with_link() { self.gpr[14] = self.gpr[15].wrapping_sub(4); }
        self.gpr[15] = self.gpr[15].wrapping_add(inst.branch_offset());
    }

    fn execute_mul_mla(&mut self, inst: ArmInstruction) {
        if inst.is_setting_flags() { return self.execute_mul_mla_s(inst); }
        let mut res = self.gpr[inst.Rs()].wrapping_mul(self.gpr[inst.Rm()]);
        if inst.is_accumulating() { res = res.wrapping_add(self.gpr[inst.Rd()]); }
        self.gpr[inst.Rn()] = res;
    }

    fn execute_mul_mla_s(&mut self, inst: ArmInstruction) {
        let mut res = (self.gpr[inst.Rs()] as u64).wrapping_mul(self.gpr[inst.Rm()] as u64);
        if inst.is_accumulating() { res = res.wrapping_add(self.gpr[inst.Rd()] as u64); }
        let x = (res & 0x00000000_FFFFFFFF_u64) as i32;
        self.gpr[inst.Rn()] = x;
        self.update_flags(x, res);
        self.cpsr.set_V(false); // Does not set V.
    }

    fn execute_mull_mlal(&mut self, inst: ArmInstruction) {
        let mut res: u64 = if inst.is_signed() {
            (self.gpr[inst.Rs()] as i64).wrapping_mul(self.gpr[inst.Rm()] as i64) as u64
        } else {
            (self.gpr[inst.Rs()] as u64).wrapping_mul(self.gpr[inst.Rm()] as u64)
        };
        if inst.is_accumulating() {
            res = res.wrapping_add(((self.gpr[inst.Rn()] as u64) << 32) | (self.gpr[inst.Rd()] as u64));
        }
        self.gpr[inst.Rn()] = ((res >> 32) & (u32::MAX as u64)) as i32;
        self.gpr[inst.Rd()] = ((res >>  0) & (u32::MAX as u64)) as i32;

        if inst.is_setting_flags() {
            self.cpsr.set_N((res as i64) < 0);
            self.cpsr.set_Z(res == 0);
            self.cpsr.set_C((res & (1 << 32)) != 0);  // Unpredictable, i.e. do what you want.
            self.cpsr.set_V(res > (u32::MAX as u64)); // Unpredictable, i.e. do what you want.
        }
    }

    fn execute_data_processing(&mut self, inst: ArmInstruction) {
        if inst.is_setting_flags() { return self.execute_data_processing_s(inst); }
        let op2: i32 = inst.calculate_shft_field(&self.gpr[..], self.cpsr.C());
        let rn: i32 = self.gpr[inst.Rn()];
        let rd: &mut i32 = &mut self.gpr[inst.Rd()];
        let c: i32 = if self.cpsr.C() { 1 } else { 0 };

        match inst.dpop() {
            ArmDPOP::AND => { *rd = rn & op2; },
            ArmDPOP::EOR => { *rd = rn ^ op2; },
            ArmDPOP::SUB => { *rd = rn.wrapping_sub(op2); },
            ArmDPOP::RSB => { *rd = op2.wrapping_sub(rn); },
            ArmDPOP::ADD => { *rd = rn.wrapping_add(op2); },
            ArmDPOP::ADC => { *rd = rn.wrapping_add(op2).wrapping_add(c) },
            ArmDPOP::SBC => { *rd = rn.wrapping_sub(op2).wrapping_sub(1-c); },
            ArmDPOP::RSC => { *rd = op2.wrapping_sub(rn).wrapping_sub(1-c); },
            ArmDPOP::TST => panic!("S bit for TST instruction not set!"),
            ArmDPOP::TEQ => panic!("S bit for TEQ instruction not set!"),
            ArmDPOP::CMP => panic!("S bit for CMP instruction not set!"),
            ArmDPOP::CMN => panic!("S bit for CMN instruction not set!"),
            ArmDPOP::ORR => { *rd = rn | op2; },
            ArmDPOP::MOV => { *rd = op2; },
            ArmDPOP::BIC => { *rd = rn & !op2; },
            ArmDPOP::MVN => { *rd = !op2; },
        }
    }

    fn execute_data_processing_s(&mut self, inst: ArmInstruction) {
        let (op2, cshft) = inst.calculate_shft_field_with_carry(&self.gpr[..], self.cpsr.C());
        let op2 = op2 as u64;
        let rn: u64 = self.gpr[inst.Rn()] as u64;
        let c: u64 = if self.cpsr.C() { 1 } else { 0 };
        // TODO
        unimplemented!()
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

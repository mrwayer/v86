use crate::cpu::cpu::*;
use crate::cpu::global_pointers::*;
use crate::paging::OrPageFault;
use crate::softfloat::{Precision, RoundingMode, F80};

use std::f64;

pub const FPU_C0: u16 = 0x100;
const FPU_C1: u16 = 0x200;
pub const FPU_C2: u16 = 0x400;
pub const FPU_C3: u16 = 0x4000;
pub const FPU_RESULT_FLAGS: u16 = FPU_C0 | FPU_C1 | FPU_C2 | FPU_C3;

pub const FPU_EX_I: u16 = 1 << 0; // invalid operation
#[allow(dead_code)]
const FPU_EX_D: u16 = 1 << 1; // denormal operand
const FPU_EX_Z: u16 = 1 << 2; // zero divide
#[allow(dead_code)]
const FPU_EX_O: u16 = 1 << 3; // overflow
const FPU_EX_U: u16 = 1 << 4; // underflow
#[allow(dead_code)]
const FPU_EX_P: u16 = 1 << 5; // precision
const FPU_EX_SF: u16 = 1 << 6;

pub fn fpu_write_st(index: i32, value: F80) {
    fpu_write_st_at(index, value, X87_SITE_ARITH)
}

/// Stores a register, leaving it in the form generated code reads inline
/// whenever the double format holds the value with nothing lost.
///
/// A helper that writes a register in the 80-bit format costs more than its own
/// call: the tag is gone, so every later inline operation on that value takes
/// its helper arm too, for the rest of the value's life. The test is integer
/// arithmetic on the two words -- no conversion is run, so the exception flags
/// the caller is about to read are untouched -- and a value it accepts reads
/// back through `fpu_canonical` as exactly the bits it came from. What the tag
/// changes is the precision later arithmetic on that value runs at, which is
/// the trade the inline path already makes for every value it produces itself;
/// configuration index 12 is the switch for both. `site` names the instruction,
/// so a report can say what the double format is still refusing.
pub fn fpu_write_st_at(index: i32, mut value: F80, site: usize) {
    dbg_assert!(index >= 0 && index < 8);
    if crate::jit::x87_tagged_more_enabled() && value.sign_exponent != FPU_RELAXED_TAG {
        unsafe {
            match crate::softfloat::f80_exact_f64_bits(&value) {
                Some(bits) => {
                    note_x87_site(X87_SITE_WRITE_EXACT);
                    value = F80 {
                        mantissa: bits,
                        sign_exponent: FPU_RELAXED_TAG,
                    };
                },
                None => {
                    note_x87_site(X87_SITE_WRITE_WIDE);
                    note_x87_site(site);
                },
            }
        }
    }
    unsafe {
        *fpu_st.offset(index as isize) = value;
    }
}

/// The tag generated code leaves in a register's exponent word when it
/// stores a double there: the mantissa word then holds the double's bits.
/// Every reader on this side turns such a register back into the 80-bit
/// format before it is used, so the arithmetic below never sees the tag.
pub const FPU_RELAXED_TAG: u16 = 0x7FFE;

/// The register as an 80-bit value, whichever form it was left in.
pub fn fpu_canonical(x: F80) -> F80 {
    if x.sign_exponent == FPU_RELAXED_TAG {
        F80::of_f64(x.mantissa)
    }
    else {
        x
    }
}

// The interpreter's arms of the forms generated code emits inline: every
// register read full and tagged, the operation done in doubles, the result
// written tagged, no conversion in between. Anything else takes the helper
// the form always took, so a stack fault is reported where the interpreter
// reported it and a value no double holds goes through the library.

/// The register at `index` as the double it holds, or nothing when it is
/// empty or in the 80-bit format -- or when configuration index 7 is off,
/// so that one switch is the control run for interpreted code as it is for
/// compiled.
unsafe fn fpu_tagged(index: i32) -> Option<f64> {
    if !crate::jit::fpu_inline_enabled() {
        return None;
    }
    let r = *fpu_st.offset(index as isize);
    if r.sign_exponent == FPU_RELAXED_TAG { Some(f64::from_bits(r.mantissa)) } else { None }
}

/// Clears the tag word of the register at `index`: every writer that marks a
/// slot empty calls this, so that a slot's sign-exponent word is never
/// `FPU_RELAXED_TAG` while `fpu_stack_empty` says it is empty, and
/// `fpu_tagged`/`gen_fpu_tag_ok` can trust the tag word alone without also
/// consulting the empty bit a pop would otherwise leave stale.
pub(crate) unsafe fn fpu_clear_tag(index: i32) {
    (*fpu_st.offset(index as isize)).sign_exponent = 0;
}

unsafe fn fpu_write_tagged(index: i32, value: f64) {
    note_x87_site(X87_SITE_INTERP_INLINE);
    *fpu_st.offset(index as isize) = F80 {
        mantissa: value.to_bits(),
        sign_exponent: FPU_RELAXED_TAG,
    };
}

/// Pushes a double tagged; a full slot is the fault `fpu_push_at` reports.
unsafe fn fpu_push_f64(value: f64, site: usize) {
    *fpu_stack_ptr = *fpu_stack_ptr - 1 & 7;
    if 0 != *fpu_stack_empty >> *fpu_stack_ptr & 1 {
        *fpu_status_word &= !FPU_C1;
        *fpu_stack_empty &= !(1 << *fpu_stack_ptr);
        fpu_write_tagged(*fpu_stack_ptr as i32, value);
    }
    else {
        *fpu_status_word |= FPU_C1;
        fpu_stack_fault();
        fpu_write_st_at(*fpu_stack_ptr as i32, F80::INDEFINITE_NAN, site);
    }
}

#[derive(Copy, Clone)]
pub enum FpuOp {
    Add,
    Mul,
    Sub,
    SubR,
    Div,
    DivR,
}

/// What an arm may compute on and produce: a normal or a zero, which is what
/// the helper's own shortcut accepts. Everything else -- a NaN, an infinity,
/// a denormal, a result that overflowed or underflowed -- raises a flag the
/// library sets and the arm would not, so such an instruction is the helper's.
fn fpu_arm_ok(x: f64) -> bool { x.is_normal() || x == 0.0 }

/// The result an arm may keep, at the precision the control word asks for:
/// the double itself when a normal or a zero, or under single precision
/// control the double narrowed to 24 bits on the terms `narrow_to_single`
/// sets; nothing for what the helper must compute instead.
fn fpu_arm_result(r: f64) -> Option<f64> {
    if crate::softfloat::precision_single() {
        crate::softfloat::narrow_to_single(r)
    }
    else if fpu_arm_ok(r) {
        Some(r)
    }
    else {
        None
    }
}

fn fpu_apply(op: FpuOp, st0: f64, other: f64) -> f64 {
    match op {
        FpuOp::Add => st0 + other,
        FpuOp::Mul => st0 * other,
        FpuOp::Sub => st0 - other,
        FpuOp::SubR => other - st0,
        FpuOp::Div => st0 / other,
        FpuOp::DivR => other / st0,
    }
}

unsafe fn fpu_op_by_helper(op: FpuOp, target: i32, val: F80) {
    match op {
        FpuOp::Add => fpu_fadd(target, val),
        FpuOp::Mul => fpu_fmul(target, val),
        FpuOp::Sub => fpu_fsub(target, val),
        FpuOp::SubR => fpu_fsubr(target, val),
        FpuOp::Div => fpu_fdiv(target, val),
        FpuOp::DivR => fpu_fdivr(target, val),
    }
}

/// `op st(0), m32`
pub unsafe fn fpu_op_m32(addr: i32, op: FpuOp) {
    let bits = return_on_pagefault!(safe_read32s(addr));
    let top = *fpu_stack_ptr as i32;
    let other = f32::from_bits(bits as u32) as f64;
    match fpu_tagged(top).map(|st0| (st0, fpu_arm_result(fpu_apply(op, st0, other)))) {
        Some((st0, Some(r))) if fpu_arm_ok(st0) && fpu_arm_ok(other) => fpu_write_tagged(top, r),
        _ => fpu_op_by_helper(op, 0, f32_to_f80(bits)),
    }
}

/// `op st(0), m64`
pub unsafe fn fpu_op_m64(addr: i32, op: FpuOp) {
    let bits = return_on_pagefault!(safe_read64s(addr));
    let top = *fpu_stack_ptr as i32;
    let other = f64::from_bits(bits);
    match fpu_tagged(top).map(|st0| (st0, fpu_arm_result(fpu_apply(op, st0, other)))) {
        Some((st0, Some(r))) if fpu_arm_ok(st0) && fpu_arm_ok(other) => fpu_write_tagged(top, r),
        _ => fpu_op_by_helper(op, 0, f64_to_f80(bits)),
    }
}

/// `op st(target), st(0) <op> st(i)`, and a pop for the `DE` forms.
pub unsafe fn fpu_op_sti(i: i32, target: i32, op: FpuOp, pop: bool) {
    let top = *fpu_stack_ptr as i32;
    match (fpu_tagged(top), fpu_tagged(top + i & 7)) {
        (Some(st0), Some(sti)) if fpu_arm_ok(st0) && fpu_arm_ok(sti) => {
            match fpu_arm_result(fpu_apply(op, st0, sti)) {
                Some(r) => fpu_write_tagged(top + target & 7, r),
                None => fpu_op_by_helper(op, target, fpu_get_sti(i)),
            }
        },
        _ => fpu_op_by_helper(op, target, fpu_get_sti(i)),
    }
    if pop {
        fpu_pop();
    }
}

/// The ordering of two doubles into C3, C2 and C0, as the inline compare
/// deposits it: a NaN on either side is unordered, and for the signalling
/// forms an invalid operation as well.
unsafe fn fpu_cmp_f64(x: f64, y: f64, signals: bool) {
    note_x87_site(X87_SITE_INTERP_INLINE);
    *fpu_status_word &= !FPU_RESULT_FLAGS;
    *fpu_status_word |= match x.partial_cmp(&y) {
        Some(std::cmp::Ordering::Greater) => 0,
        Some(std::cmp::Ordering::Less) => FPU_C0,
        Some(std::cmp::Ordering::Equal) => FPU_C3,
        None => FPU_C0 | FPU_C2 | FPU_C3 | if signals { FPU_EX_I } else { 0 },
    };
}

/// `fcom m32` / `fcomp m32`
pub unsafe fn fpu_fcom_m32(addr: i32, pop: bool) {
    let bits = return_on_pagefault!(safe_read32s(addr));
    match fpu_tagged(*fpu_stack_ptr as i32) {
        Some(st0) => fpu_cmp_f64(st0, f32::from_bits(bits as u32) as f64, true),
        None => fpu_fcom(f32_to_f80(bits)),
    }
    if pop {
        fpu_pop();
    }
}

/// `fcom m64` / `fcomp m64`
pub unsafe fn fpu_fcom_m64(addr: i32, pop: bool) {
    let bits = return_on_pagefault!(safe_read64s(addr));
    match fpu_tagged(*fpu_stack_ptr as i32) {
        Some(st0) => fpu_cmp_f64(st0, f64::from_bits(bits), true),
        None => fpu_fcom(f64_to_f80(bits)),
    }
    if pop {
        fpu_pop();
    }
}

/// A compare against st(i), signalling or quiet, followed by `pops` pops.
pub unsafe fn fpu_fcom_sti(i: i32, pops: u32, quiet: bool) {
    let top = *fpu_stack_ptr as i32;
    match (fpu_tagged(top), fpu_tagged(top + i & 7)) {
        (Some(x), Some(y)) => fpu_cmp_f64(x, y, !quiet),
        _ if quiet => fpu_fucom(i),
        _ => fpu_fcom(fpu_get_sti(i)),
    }
    for _ in 0..pops {
        fpu_pop();
    }
}

pub unsafe fn fpu_get_st0() -> F80 {
    dbg_assert!(*fpu_stack_ptr < 8);
    if 0 != *fpu_stack_empty >> *fpu_stack_ptr & 1 {
        *fpu_status_word &= !FPU_C1;
        fpu_stack_fault();
        return F80::INDEFINITE_NAN;
    }
    else {
        return fpu_canonical(*fpu_st.offset(*fpu_stack_ptr as isize));
    };
}
pub unsafe fn fpu_stack_fault() {
    // TODO: Interrupt
    *fpu_status_word |= FPU_EX_SF | FPU_EX_I;
}

pub unsafe fn fpu_zero_fault() {
    // TODO: Interrupt
    *fpu_status_word |= FPU_EX_Z;
}

pub unsafe fn fpu_underflow_fault() {
    // TODO: Interrupt
    *fpu_status_word |= FPU_EX_U;
}

pub unsafe fn fpu_sti_empty(mut i: i32) -> bool {
    dbg_assert!(i >= 0 && i < 8);
    i = i + *fpu_stack_ptr as i32 & 7;
    return 0 != *fpu_stack_empty >> i & 1;
}

#[no_mangle]
pub unsafe fn fpu_get_sti_jit(dst: *mut F80, i: i32) { *dst = fpu_get_sti(i); }

pub unsafe fn fpu_get_sti(mut i: i32) -> F80 {
    dbg_assert!(i >= 0 && i < 8);
    i = i + *fpu_stack_ptr as i32 & 7;
    if 0 != *fpu_stack_empty >> i & 1 {
        *fpu_status_word &= !FPU_C1;
        fpu_stack_fault();
        return F80::INDEFINITE_NAN;
    }
    else {
        return fpu_canonical(*fpu_st.offset(i as isize));
    };
}

// only used for debugging
#[no_mangle]
pub unsafe fn fpu_get_sti_f64(mut i: i32) -> f64 {
    i = i + *fpu_stack_ptr as i32 & 7;
    f64::from_bits(fpu_canonical(*fpu_st.offset(i as isize)).to_f64())
}

#[no_mangle]
pub unsafe fn f32_to_f80_jit(dst: *mut F80, v: i32) { *dst = f32_to_f80(v) }
pub unsafe fn f32_to_f80(v: i32) -> F80 {
    F80::clear_exception_flags();
    let x = F80::of_f32(v);
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}
#[no_mangle]
pub unsafe fn f64_to_f80_jit(dst: *mut F80, v: u64) { *dst = f64_to_f80(v) }
pub unsafe fn f64_to_f80(v: u64) -> F80 {
    F80::clear_exception_flags();
    let x = F80::of_f64(v);
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}
#[no_mangle]
pub unsafe fn f80_to_f32(v: F80) -> i32 {
    F80::clear_exception_flags();
    let x = v.to_f32();
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}
#[no_mangle]
pub unsafe fn f80_to_f64(v: F80) -> u64 {
    F80::clear_exception_flags();
    let x = v.to_f64();
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}

#[no_mangle]
pub unsafe fn i32_to_f80_jit(dst: *mut F80, v: i32) { *dst = i32_to_f80(v) }
pub unsafe fn i32_to_f80(v: i32) -> F80 { F80::of_i32(v) }
#[no_mangle]
pub unsafe fn i64_to_f80_jit(dst: *mut F80, v: i64) { *dst = i64_to_f80(v) }
pub unsafe fn i64_to_f80(v: i64) -> F80 { F80::of_i64(v) }

pub unsafe fn fpu_load_i16(addr: i32) -> OrPageFault<F80> {
    let v = safe_read16(addr)? as i16 as i32;
    Ok(F80::of_i32(v))
}
pub unsafe fn fpu_load_i32(addr: i32) -> OrPageFault<F80> {
    let v = safe_read32s(addr)?;
    Ok(F80::of_i32(v))
}
pub unsafe fn fpu_load_i64(addr: i32) -> OrPageFault<F80> {
    let v = safe_read64s(addr)? as i64;
    Ok(F80::of_i64(v))
}

pub unsafe fn fpu_load_m80(addr: i32) -> OrPageFault<F80> {
    let mantissa = safe_read64s(addr)?;
    let sign_exponent = safe_read16(addr + 8)? as u16;
    // TODO: Canonical form
    Ok(F80 {
        mantissa,
        sign_exponent,
    })
}

#[no_mangle]
pub unsafe fn fpu_load_status_word() -> u16 {
    dbg_assert!(*fpu_stack_ptr < 8);
    return *fpu_status_word & !(7 << 11) | (*fpu_stack_ptr as u16) << 11;
}
#[no_mangle]
pub unsafe fn fpu_fadd(target_index: i32, val: F80) {
    F80::clear_exception_flags();
    let st0 = fpu_get_st0();
    fpu_write_st(*fpu_stack_ptr as i32 + target_index & 7, st0.add_arith(val));
    *fpu_status_word |= F80::get_exception_flags() as u16;
}
pub unsafe fn fpu_fclex() { *fpu_status_word = 0; }
pub unsafe fn fpu_fcmovcc(condition: bool, r: i32) {
    // outside of the condition is correct: A stack fault happens even if the condition is not
    // fulfilled
    let x = fpu_get_sti(r);
    if fpu_sti_empty(r) {
        fpu_write_st_at(*fpu_stack_ptr as i32, F80::INDEFINITE_NAN, X87_SITE_FCMOVCC)
    }
    else {
        if condition {
            fpu_write_st_at(*fpu_stack_ptr as i32, x, X87_SITE_FCMOVCC);
            *fpu_stack_empty &= !(1 << *fpu_stack_ptr)
        };
    }
}

#[no_mangle]
pub unsafe fn fpu_fcom(y: F80) {
    F80::clear_exception_flags();
    let x = fpu_get_st0();
    *fpu_status_word &= !FPU_RESULT_FLAGS;
    match x.partial_cmp(&y) {
        Some(std::cmp::Ordering::Greater) => {},
        Some(std::cmp::Ordering::Less) => *fpu_status_word |= FPU_C0,
        Some(std::cmp::Ordering::Equal) => *fpu_status_word |= FPU_C3,
        None => *fpu_status_word |= FPU_C0 | FPU_C2 | FPU_C3,
    }
    *fpu_status_word |= F80::get_exception_flags() as u16;
}

#[no_mangle]
pub unsafe fn fpu_fcomi(r: i32) {
    F80::clear_exception_flags();
    let x = fpu_get_st0();
    let y = fpu_get_sti(r);
    *flags_changed = 0;
    *flags &= !FLAGS_ALL;
    match x.partial_cmp(&y) {
        Some(std::cmp::Ordering::Greater) => {},
        Some(std::cmp::Ordering::Less) => *flags |= 1,
        Some(std::cmp::Ordering::Equal) => *flags |= FLAG_ZERO,
        None => *flags |= 1 | FLAG_PARITY | FLAG_ZERO,
    }
    *fpu_status_word |= F80::get_exception_flags() as u16;
}

#[no_mangle]
pub unsafe fn fpu_fcomip(r: i32) {
    fpu_fcomi(r);
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_pop() {
    dbg_assert!(*fpu_stack_ptr < 8);
    *fpu_stack_empty |= 1 << *fpu_stack_ptr;
    fpu_clear_tag(*fpu_stack_ptr as i32);
    *fpu_stack_ptr = *fpu_stack_ptr + 1 & 7;
}

#[no_mangle]
pub unsafe fn fpu_fcomp(val: F80) {
    fpu_fcom(val);
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_fdiv(target_index: i32, val: F80) {
    F80::clear_exception_flags();
    let st0 = fpu_get_st0();
    fpu_write_st(*fpu_stack_ptr as i32 + target_index & 7, st0.div_arith(val));
    *fpu_status_word |= F80::get_exception_flags() as u16;
}
#[no_mangle]
pub unsafe fn fpu_fdivr(target_index: i32, val: F80) {
    F80::clear_exception_flags();
    let st0 = fpu_get_st0();
    fpu_write_st(*fpu_stack_ptr as i32 + target_index & 7, val.div_arith(st0));
    *fpu_status_word |= F80::get_exception_flags() as u16;
}
#[no_mangle]
pub unsafe fn fpu_ffree(r: i32) {
    let index = *fpu_stack_ptr as i32 + r & 7;
    *fpu_stack_empty |= 1 << index;
    fpu_clear_tag(index);
}

pub unsafe fn fpu_fildm16(addr: i32) {
    fpu_push_at(return_on_pagefault!(fpu_load_i16(addr)), X87_SITE_FILD_M16)
}
pub unsafe fn fpu_fildm32(addr: i32) {
    fpu_push_at(return_on_pagefault!(fpu_load_i32(addr)), X87_SITE_FILD_M32)
}
pub unsafe fn fpu_fildm64(addr: i32) {
    fpu_push_at(return_on_pagefault!(fpu_load_i64(addr)), X87_SITE_FILD_M64)
}

#[no_mangle]
pub unsafe fn fpu_push(x: F80) { fpu_push_at(x, X87_SITE_PUSH_OTHER) }
pub unsafe fn fpu_push_at(x: F80, site: usize) {
    *fpu_stack_ptr = *fpu_stack_ptr - 1 & 7;
    if 0 != *fpu_stack_empty >> *fpu_stack_ptr & 1 {
        *fpu_status_word &= !FPU_C1;
        *fpu_stack_empty &= !(1 << *fpu_stack_ptr);
        fpu_write_st_at(*fpu_stack_ptr as i32, x, site);
    }
    else {
        *fpu_status_word |= FPU_C1;
        fpu_stack_fault();
        fpu_write_st_at(*fpu_stack_ptr as i32, F80::INDEFINITE_NAN, site);
    };
}
pub unsafe fn fpu_finit() {
    set_control_word(0x37F);
    *fpu_status_word = 0;
    *fpu_ip = 0;
    *fpu_dp = 0;
    *fpu_opcode = 0;
    *fpu_stack_empty = 0xFF;
    for i in 0..8 {
        fpu_clear_tag(i);
    }
    *fpu_stack_ptr = 0;
}

#[no_mangle]
pub unsafe fn set_control_word(cw: u16) {
    *fpu_control_word = cw;

    let rc = cw >> 10 & 3;
    F80::set_rounding_mode(match rc {
        0 => RoundingMode::NearEven,
        1 => RoundingMode::Floor,
        2 => RoundingMode::Ceil,
        3 => RoundingMode::Trunc,
        _ => {
            dbg_assert!(false);
            RoundingMode::NearEven
        },
    });

    let precision_control = cw >> 8 & 3;
    F80::set_precision(match precision_control {
        0 => Precision::P32,
        1 => Precision::P80, // undefined
        2 => Precision::P64,
        3 => Precision::P80,
        _ => {
            dbg_assert!(false);
            Precision::P80
        },
    });
    // Generated code reads this rather than the control word, one byte
    // against a load, a mask and a compare on every inline arithmetic result.
    *fpu_precision_single = (precision_control == 0) as u8;
}

pub unsafe fn fpu_invalid_arithmetic() { *fpu_status_word |= FPU_EX_I; }

#[no_mangle]
pub unsafe fn fpu_convert_to_i16(f: F80) -> i16 {
    let st0 = fpu_convert_to_i32(f);
    if st0 < -0x8000 || st0 > 0x7FFF {
        fpu_invalid_arithmetic();
        -0x8000
    }
    else {
        st0 as i16
    }
}
pub unsafe fn fpu_fistm16(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 2));
    let v = fpu_convert_to_i16(fpu_get_st0());
    safe_write16(addr, v as i32 & 0xFFFF).unwrap();
}
pub unsafe fn fpu_fistm16p(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 2));
    let v = fpu_convert_to_i16(fpu_get_st0());
    safe_write16(addr, v as i32 & 0xFFFF).unwrap();
    fpu_pop();
}
#[no_mangle]
pub unsafe fn fpu_truncate_to_i16(f: F80) -> i16 {
    let st0 = fpu_truncate_to_i32(f);
    if st0 < -0x8000 || st0 > 0x7FFF {
        fpu_invalid_arithmetic();
        -0x8000
    }
    else {
        st0 as i16
    }
}
pub unsafe fn fpu_fisttpm16(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 2));
    let v = fpu_truncate_to_i16(fpu_get_st0());
    safe_write16(addr, v as i32 & 0xFFFF).unwrap();
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_convert_to_i32(f: F80) -> i32 {
    F80::clear_exception_flags();
    let x = f.to_i32();
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}
pub unsafe fn fpu_fistm32(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 4));
    let v = fpu_convert_to_i32(fpu_get_st0());
    safe_write32(addr, v).unwrap();
}
pub unsafe fn fpu_fistm32p(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 4));
    let v = fpu_convert_to_i32(fpu_get_st0());
    safe_write32(addr, v).unwrap();
    fpu_pop();
}
#[no_mangle]
pub unsafe fn fpu_truncate_to_i32(f: F80) -> i32 {
    F80::clear_exception_flags();
    let x = f.truncate_to_i32();
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}
pub unsafe fn fpu_fisttpm32(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 4));
    let v = fpu_truncate_to_i32(fpu_get_st0());
    safe_write32(addr, v).unwrap();
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_convert_to_i64(f: F80) -> i64 {
    F80::clear_exception_flags();
    let x = f.to_i64();
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}
pub unsafe fn fpu_fistm64p(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 8));
    let v = fpu_convert_to_i64(fpu_get_st0());
    safe_write64(addr, v as u64).unwrap();
    fpu_pop();
}
#[no_mangle]
pub unsafe fn fpu_truncate_to_i64(f: F80) -> i64 {
    F80::clear_exception_flags();
    let x = f.truncate_to_i64();
    *fpu_status_word |= F80::get_exception_flags() as u16;
    x
}
pub unsafe fn fpu_fisttpm64(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 8));
    let v = fpu_truncate_to_i64(fpu_get_st0());
    safe_write64(addr, v as u64).unwrap();
    fpu_pop();
}

pub unsafe fn fpu_fldcw(addr: i32) {
    let word = return_on_pagefault!(safe_read16(addr)) as u16;
    set_control_word(word);
}

#[no_mangle]
pub unsafe fn fpu_fldenv16(addr: i32) {
    // protected-mode layout (the real-mode layout stores the linear ip/dp)
    set_control_word(safe_read16(addr).unwrap() as u16);
    fpu_set_status_word(safe_read16(addr + 2).unwrap() as u16);
    fpu_set_tag_word(safe_read16(addr + 4).unwrap());
    *fpu_ip = safe_read16(addr + 6).unwrap();
    *fpu_ip_selector = safe_read16(addr + 8).unwrap();
    *fpu_dp = safe_read16(addr + 10).unwrap();
    *fpu_dp_selector = safe_read16(addr + 12).unwrap()
}
#[no_mangle]
pub unsafe fn fpu_fldenv32(addr: i32) {
    // protected-mode layout (the real-mode layout stores the linear ip/dp)
    set_control_word(safe_read16(addr).unwrap() as u16);
    fpu_set_status_word(safe_read16(addr + 4).unwrap() as u16);
    fpu_set_tag_word(safe_read16(addr + 8).unwrap());
    *fpu_ip = safe_read32s(addr + 12).unwrap();
    *fpu_ip_selector = safe_read16(addr + 16).unwrap();
    *fpu_opcode = safe_read16(addr + 18).unwrap();
    *fpu_dp = safe_read32s(addr + 20).unwrap();
    *fpu_dp_selector = safe_read16(addr + 24).unwrap()
}
pub unsafe fn fpu_unimpl() {
    dbg_assert!(false);
    trigger_ud();
}
pub unsafe fn fpu_set_tag_word(tag_word: i32) {
    *fpu_stack_empty = 0;
    for i in 0..8 {
        let empty = tag_word >> (2 * i) & 3 == 3;
        *fpu_stack_empty |= (empty as u8) << i;
        if empty {
            fpu_clear_tag(i);
        }
    }
}
pub unsafe fn fpu_set_status_word(sw: u16) {
    *fpu_status_word = sw & !(7 << 11);
    *fpu_stack_ptr = (sw >> 11 & 7) as u8;
}

// A load from memory takes the arm on what the arithmetic arm takes, a
// normal or a zero; anything else is the library's, which is where an SNaN
// source raises the invalid operation and becomes the QNaN the register
// holds, and where a NaN, an infinity or a denormal stays in the 80-bit
// format rather than tagged.
pub unsafe fn fpu_fldm32(addr: i32) {
    let bits = return_on_pagefault!(safe_read32s(addr));
    let single = f32::from_bits(bits as u32);
    if crate::jit::fpu_inline_enabled() && (single.is_normal() || single == 0.0) {
        fpu_push_f64(single as f64, X87_SITE_FLD_M32)
    }
    else {
        fpu_push_at(f32_to_f80(bits), X87_SITE_FLD_M32)
    }
}
pub unsafe fn fpu_fldm64(addr: i32) {
    let bits = return_on_pagefault!(safe_read64s(addr));
    let double = f64::from_bits(bits);
    if crate::jit::fpu_inline_enabled() && fpu_arm_ok(double) {
        fpu_push_f64(double, X87_SITE_FLD_M64)
    }
    else {
        fpu_push_at(f64_to_f80(bits), X87_SITE_FLD_M64)
    }
}
pub unsafe fn fpu_fldm80(addr: i32) {
    fpu_push_at(return_on_pagefault!(fpu_load_m80(addr)), X87_SITE_FLD_M80)
}
#[no_mangle]
pub unsafe fn fpu_fldm80_without_fault(addr: i32) {
    fpu_push_at(fpu_load_m80(addr).unwrap(), X87_SITE_FLD_M80)
}

#[no_mangle]
pub unsafe fn fpu_fmul(target_index: i32, val: F80) {
    let st0 = fpu_get_st0();
    fpu_write_st(*fpu_stack_ptr as i32 + target_index & 7, st0.mul_arith(val));
}
pub unsafe fn fpu_fnstsw_mem(addr: i32) {
    return_on_pagefault!(safe_write16(addr, fpu_load_status_word().into()));
}
pub unsafe fn fpu_fnstsw_reg() { write_reg16(AX, fpu_load_status_word().into()); }
pub unsafe fn fpu_fprem(ieee: bool) {
    // false: Faster, fails nasmtests
    // true: Slower, fails qemutests
    let intel_compatibility = false;

    let st0 = fpu_get_st0();
    let st1 = fpu_get_sti(1);

    if st1 == F80::ZERO {
        if st0 == F80::ZERO {
            fpu_invalid_arithmetic();
        }
        else {
            fpu_zero_fault();
        }
        fpu_write_st_at(*fpu_stack_ptr as i32, F80::INDEFINITE_NAN, X87_SITE_FPREM);
        return;
    }

    let exp0 = st0.log2();
    let exp1 = st1.log2();
    let d = (exp0 - exp1).abs();
    if !intel_compatibility || d < F80::of_f64(f64::to_bits(64.0)) {
        let fprem_quotient = if ieee { (st0 / st1).round() } else { (st0 / st1).trunc() };
        fpu_write_st_at(*fpu_stack_ptr as i32, st0 - st1 * fprem_quotient, X87_SITE_FPREM);
        let fprem_quotient = fprem_quotient.to_i32();
        *fpu_status_word &= !(FPU_C0 | FPU_C1 | FPU_C3);
        if 0 != fprem_quotient & 1 {
            *fpu_status_word |= FPU_C1
        }
        if 0 != fprem_quotient & 1 << 1 {
            *fpu_status_word |= FPU_C3
        }
        if 0 != fprem_quotient & 1 << 2 {
            *fpu_status_word |= FPU_C0
        }
        *fpu_status_word &= !FPU_C2;
    }
    else {
        let n = F80::of_f64(f64::to_bits(32.0));
        let fprem_quotient =
            (if ieee { (st0 / st1).round() } else { (st0 / st1).trunc() } / (d - n).two_pow());
        fpu_write_st_at(
            *fpu_stack_ptr as i32,
            st0 - st1 * fprem_quotient * (d - n).two_pow(),
            X87_SITE_FPREM,
        );
        *fpu_status_word |= FPU_C2;
    }
}

pub unsafe fn fpu_frstor16(_addr: i32) {
    dbg_log!("frstor16");
    fpu_unimpl();
}
pub unsafe fn fpu_frstor32(mut addr: i32) {
    return_on_pagefault!(readable_or_pagefault(addr, 28 + 8 * 10));
    fpu_fldenv32(addr);
    addr += 28;
    for i in 0..8 {
        let reg_index = *fpu_stack_ptr as i32 + i & 7;
        fpu_write_st_at(reg_index, fpu_load_m80(addr).unwrap(), X87_SITE_FRSTOR);
        // The disk image holds real bits for every register regardless of
        // its tag, and fpu_write_st_at may re-tag a value that happens to
        // round-trip through a double exactly -- fpu_fldenv32 already
        // decided which registers are empty, and this write must not
        // contradict it.
        if 0 != *fpu_stack_empty >> reg_index & 1 {
            fpu_clear_tag(reg_index);
        }
        addr += 10;
    }
}

pub unsafe fn fpu_fsave16(_addr: i32) {
    dbg_log!("fsave16");
    fpu_unimpl();
}
pub unsafe fn fpu_fsave32(mut addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 108));
    fpu_fstenv32(addr);
    addr += 28;
    for i in 0..8 {
        let reg_index = i + *fpu_stack_ptr as i32 & 7;
        fpu_store_m80(addr, fpu_canonical(*fpu_st.offset(reg_index as isize)));
        addr += 10;
    }
    fpu_finit();
}

pub unsafe fn fpu_store_m80(addr: i32, f: F80) {
    // writable_or_pagefault must have checked called by the caller!
    safe_write64(addr, f.mantissa).unwrap();
    safe_write16(addr + 8, f.sign_exponent as i32).unwrap();
}

#[no_mangle]
pub unsafe fn fpu_fstenv16(addr: i32) {
    safe_write16(addr + 0, *fpu_control_word as i32).unwrap();
    safe_write16(addr + 2, fpu_load_status_word() as i32).unwrap();
    safe_write16(addr + 4, fpu_load_tag_word()).unwrap();
    safe_write16(addr + 6, *fpu_ip & 0xFFFF).unwrap();
    safe_write16(addr + 8, *fpu_ip_selector).unwrap();
    safe_write16(addr + 10, *fpu_dp & 0xFFFF).unwrap();
    safe_write16(addr + 12, *fpu_dp_selector).unwrap();
}

#[no_mangle]
pub unsafe fn fpu_fstenv32(addr: i32) {
    let high_bits = 0xFFFF0000u32 as i32;
    safe_write32(addr + 0, high_bits + *fpu_control_word as i32).unwrap();
    safe_write32(addr + 4, high_bits + fpu_load_status_word() as i32).unwrap();
    safe_write32(addr + 8, high_bits + fpu_load_tag_word()).unwrap();
    safe_write32(addr + 12, *fpu_ip).unwrap();
    safe_write16(addr + 16, *fpu_ip_selector).unwrap();
    safe_write16(addr + 18, *fpu_opcode).unwrap();
    safe_write32(addr + 20, *fpu_dp).unwrap();
    safe_write32(addr + 24, high_bits | *fpu_dp_selector).unwrap();
}
#[no_mangle]
pub unsafe fn fpu_load_tag_word() -> i32 {
    let mut tag_word = 0;
    for i in 0..8 {
        let value = fpu_canonical(*fpu_st.offset(i as isize));
        if 0 != *fpu_stack_empty >> i & 1 {
            tag_word |= 3 << (i << 1)
        }
        else if value == F80::ZERO {
            tag_word |= 1 << (i << 1)
        }
        else if !value.is_finite() {
            tag_word |= 2 << (i << 1)
        }
    }
    return tag_word;
}
#[no_mangle]
pub unsafe fn fpu_fst(r: i32) {
    let index: i32 = *fpu_stack_ptr as i32 + r & 7;
    fpu_write_st_at(index, fpu_get_st0(), X87_SITE_FST_STI);
    if 0 == ((*fpu_stack_empty >> *fpu_stack_ptr) & 1) {
        *fpu_stack_empty &= !(1 << index);
    }
}
pub unsafe fn fpu_fst80p(addr: i32) {
    return_on_pagefault!(writable_or_pagefault(addr, 10));
    fpu_store_m80(addr, fpu_get_st0());
    fpu_pop();
}

pub unsafe fn fpu_fstcw(addr: i32) {
    return_on_pagefault!(safe_write16(addr, (*fpu_control_word).into()));
}

pub unsafe fn fpu_fstm32(addr: i32) { return_on_pagefault!(fpu_store_st0_m32(addr)) }
/// st(0) narrowed to a single: a tagged register as generated code narrows
/// it, when that single is a normal or the zero of a zero; any other through
/// the library, which is where an overflow or an underflow raises its flag.
unsafe fn fpu_store_st0_m32(addr: i32) -> OrPageFault<()> {
    match fpu_tagged(*fpu_stack_ptr as i32).map(|st0| (st0, st0 as f32)) {
        Some((st0, s)) if fpu_arm_ok(st0) && (st0 == 0.0 || s.is_normal()) => {
            note_x87_site(X87_SITE_INTERP_INLINE);
            safe_write32(addr, s.to_bits() as i32)
        },
        _ => fpu_store_m32(addr, fpu_get_st0()),
    }
}
pub unsafe fn fpu_store_m32(addr: i32, x: F80) -> OrPageFault<()> {
    F80::clear_exception_flags();
    safe_write32(addr, x.to_f32())?;
    *fpu_status_word |= F80::get_exception_flags() as u16;
    Ok(())
}
pub unsafe fn fpu_fstm32p(addr: i32) {
    return_on_pagefault!(fpu_store_st0_m32(addr));
    fpu_pop();
}
pub unsafe fn fpu_fstm64(addr: i32) { return_on_pagefault!(fpu_store_st0_m64(addr)) }
unsafe fn fpu_store_st0_m64(addr: i32) -> OrPageFault<()> {
    match fpu_tagged(*fpu_stack_ptr as i32) {
        Some(st0) => {
            note_x87_site(X87_SITE_INTERP_INLINE);
            safe_write64(addr, st0.to_bits())
        },
        None => fpu_store_m64(addr, fpu_get_st0()),
    }
}
pub unsafe fn fpu_store_m64(addr: i32, x: F80) -> OrPageFault<()> { safe_write64(addr, x.to_f64()) }
pub unsafe fn fpu_fstm64p(addr: i32) {
    // XXX: writable_or_pagefault before get_st0
    return_on_pagefault!(fpu_store_st0_m64(addr));
    fpu_pop();
}
#[no_mangle]
pub unsafe fn fpu_fstp(r: i32) {
    fpu_fst(r);
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_fbstp(addr: i32) {
    let st0 = fpu_get_st0();
    let mut x = st0.to_i64().unsigned_abs();
    if x <= 99_9999_9999_9999_9999 {
        for i in 0..=8 {
            let low = x % 10;
            x /= 10;
            let high = x % 10;
            x /= 10;
            safe_write8(addr + i, (high as i32) << 4 | low as i32).unwrap();
        }
        safe_write8(addr + 9, if st0.sign() { 0x80 } else { 0 }).unwrap();
    }
    else {
        fpu_invalid_arithmetic();
        safe_write64(addr + 0, 0xC000_0000_0000_0000).unwrap();
        safe_write16(addr + 8, 0xFFFF).unwrap();
    }
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_fsub(target_index: i32, val: F80) {
    let st0 = fpu_get_st0();
    fpu_write_st(*fpu_stack_ptr as i32 + target_index & 7, st0.sub_arith(val))
}
#[no_mangle]
pub unsafe fn fpu_fsubr(target_index: i32, val: F80) {
    let st0 = fpu_get_st0();
    fpu_write_st(*fpu_stack_ptr as i32 + target_index & 7, val.sub_arith(st0))
}

pub unsafe fn fpu_ftst() {
    let x = fpu_get_st0();
    *fpu_status_word &= !FPU_RESULT_FLAGS;
    if x.is_nan() {
        *fpu_status_word |= FPU_C3 | FPU_C2 | FPU_C0
    }
    else if x == F80::ZERO {
        *fpu_status_word |= FPU_C3
    }
    else if x < F80::ZERO {
        *fpu_status_word |= FPU_C0
    }
    // TODO: unordered (x is nan, etc)
}

#[no_mangle]
pub unsafe fn fpu_fucom(r: i32) {
    F80::clear_exception_flags();
    let x = fpu_get_st0();
    let y = fpu_get_sti(r);
    *fpu_status_word &= !FPU_RESULT_FLAGS;
    match x.partial_cmp_quiet(&y) {
        Some(std::cmp::Ordering::Greater) => {},
        Some(std::cmp::Ordering::Less) => *fpu_status_word |= FPU_C0,
        Some(std::cmp::Ordering::Equal) => *fpu_status_word |= FPU_C3,
        None => *fpu_status_word |= FPU_C0 | FPU_C2 | FPU_C3,
    }
    *fpu_status_word |= F80::get_exception_flags() as u16;
}

#[no_mangle]
pub unsafe fn fpu_fucomi(r: i32) {
    F80::clear_exception_flags();
    let x = fpu_get_st0();
    let y = fpu_get_sti(r);
    *flags_changed = 0;
    *flags &= !FLAGS_ALL;
    match x.partial_cmp_quiet(&y) {
        Some(std::cmp::Ordering::Greater) => {},
        Some(std::cmp::Ordering::Less) => *flags |= 1,
        Some(std::cmp::Ordering::Equal) => *flags |= FLAG_ZERO,
        None => *flags |= 1 | FLAG_PARITY | FLAG_ZERO,
    }
    *fpu_status_word |= F80::get_exception_flags() as u16;
}

#[no_mangle]
pub unsafe fn fpu_fucomip(r: i32) {
    fpu_fucomi(r);
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_fucomp(r: i32) {
    fpu_fucom(r);
    fpu_pop();
}

#[no_mangle]
pub unsafe fn fpu_fucompp() {
    fpu_fucom(1);
    fpu_pop();
    fpu_pop();
}

pub unsafe fn fpu_fxam() {
    let x = fpu_get_st0();
    *fpu_status_word &= !FPU_RESULT_FLAGS;
    *fpu_status_word |= (x.sign() as u16) << 9;
    if 0 != *fpu_stack_empty >> *fpu_stack_ptr & 1 {
        *fpu_status_word |= FPU_C3 | FPU_C0
    }
    else if x.is_nan() {
        *fpu_status_word |= FPU_C0
    }
    else if x == F80::ZERO {
        *fpu_status_word |= FPU_C3
    }
    else if !x.is_finite() {
        *fpu_status_word |= FPU_C2 | FPU_C0
    }
    else {
        *fpu_status_word |= FPU_C2
    }
    // TODO:
    // Unsupported, Denormal
}

#[no_mangle]
pub unsafe fn fpu_fxch(i: i32) {
    let sti = fpu_get_sti(i);
    fpu_write_st_at(*fpu_stack_ptr as i32 + i & 7, fpu_get_st0(), X87_SITE_FXCH);
    fpu_write_st_at(*fpu_stack_ptr as i32, sti, X87_SITE_FXCH);
}
pub unsafe fn fpu_fyl2x() {
    let st0 = fpu_get_st0();
    if st0 < F80::ZERO {
        fpu_invalid_arithmetic();
    }
    else if st0 == F80::ZERO {
        fpu_zero_fault();
    }
    fpu_write_st_at(
        *fpu_stack_ptr as i32 + 1 & 7,
        fpu_get_sti(1) * st0.ln() / F80::LN_2,
        X87_SITE_TRANSCENDENTAL,
    );
    fpu_pop();
}

pub unsafe fn fpu_fxtract() {
    let st0 = fpu_get_st0();
    if st0 == F80::ZERO {
        fpu_zero_fault();
        fpu_write_st_at(*fpu_stack_ptr as i32, F80::NEG_INFINITY, X87_SITE_TRANSCENDENTAL);
        fpu_push(st0);
    }
    else {
        let exp = st0.exponent();
        fpu_write_st_at(
            *fpu_stack_ptr as i32,
            F80::of_i32(exp.into()),
            X87_SITE_TRANSCENDENTAL,
        );
        fpu_push(F80 {
            sign_exponent: 0x3FFF,
            mantissa: st0.mantissa,
        });
    }
}

pub unsafe fn fwait() {
    // NOP unless FPU instructions run in parallel with CPU instructions
}

pub unsafe fn fpu_fchs() {
    let st0 = fpu_get_st0();
    fpu_write_st_at(*fpu_stack_ptr as i32, -st0, X87_SITE_SIGN);
}

pub unsafe fn fpu_fabs() {
    let st0 = fpu_get_st0();
    fpu_write_st_at(*fpu_stack_ptr as i32, st0.abs(), X87_SITE_SIGN);
}

pub unsafe fn fpu_f2xm1() {
    let st0 = fpu_get_st0();
    let r = st0.two_pow() - F80::ONE;
    fpu_write_st_at(*fpu_stack_ptr as i32, r, X87_SITE_TRANSCENDENTAL)
}

pub unsafe fn fpu_fptan() {
    let st0 = fpu_get_st0();
    //if -pow(2.0, 63.0) < st0 && st0 < pow(2.0, 63.0) {
    fpu_write_st_at(*fpu_stack_ptr as i32, st0.tan(), X87_SITE_TRANSCENDENTAL);
    // no bug: push constant 1
    fpu_push(F80::ONE);
    *fpu_status_word &= !FPU_C2;
    //}
    //else {
    //    *fpu_status_word |= FPU_C2;
    //}
}

pub unsafe fn fpu_fpatan() {
    let st0 = fpu_get_st0();
    let st1 = fpu_get_sti(1);
    fpu_write_st_at(*fpu_stack_ptr as i32 + 1 & 7, st1.atan2(st0), X87_SITE_TRANSCENDENTAL);
    fpu_pop();
}

pub unsafe fn fpu_fyl2xp1() {
    // fyl2xp1: y * log2(x+1) and pop
    let st0 = fpu_get_st0();
    let st1 = fpu_get_sti(1);
    let y = st1 * (st0 + F80::ONE).ln() / F80::LN_2;
    fpu_write_st_at(*fpu_stack_ptr as i32 + 1 & 7, y, X87_SITE_TRANSCENDENTAL);
    fpu_pop();
}

pub unsafe fn fpu_fsqrt() {
    let st0 = fpu_get_st0();
    //if st0 < 0.0 {
    //    fpu_invalid_arithmetic();
    //}
    fpu_write_st_at(*fpu_stack_ptr as i32, st0.sqrt(), X87_SITE_FSQRT)
}

pub unsafe fn fpu_fsincos() {
    let st0 = fpu_get_st0();
    //if pow(-2.0, 63.0) < st0 && st0 < pow(2.0, 63.0) {
    fpu_write_st_at(*fpu_stack_ptr as i32, st0.sin(), X87_SITE_TRANSCENDENTAL);
    fpu_push(st0.cos());
    *fpu_status_word &= !FPU_C2;
    //}
    //else {
    //    *fpu_status_word |= FPU_C2;
    //}
}

pub unsafe fn fpu_frndint() {
    let st0 = fpu_get_st0();
    fpu_write_st_at(*fpu_stack_ptr as i32, st0.round(), X87_SITE_FRNDINT);
}

/// `fscale`: st(0) scaled by two to the integer part of st(1).
///
/// A normal is scaled on its exponent field, so its mantissa is untouched
/// whatever the precision control says -- the instruction is not one the
/// control word rounds -- and the range is the format's own, to 2^16383,
/// which a double the scale was once computed through does not reach.
/// Everything else -- a zero, an infinity, a NaN, a denormal, or a result
/// that leaves the exponent range -- is a multiply by a power of two the
/// library carries out, in as many steps as its exponent range needs, so
/// that the value, the overflow or underflow and the flags are the library's.
pub unsafe fn fpu_fscale() {
    let st0 = fpu_get_st0();
    let st1 = fpu_get_sti(1);
    let n = f64::from_bits(st1.trunc().to_f64());
    let exponent = (st0.sign_exponent & 0x7FFF) as i32;
    let scaled = exponent as f64 + n;
    let y = if exponent != 0
        && exponent != 0x7FFF
        && st0.mantissa >> 63 != 0
        && n.is_finite()
        && scaled >= 1.0
        && scaled <= 0x7FFE as f64
    {
        F80 {
            mantissa: st0.mantissa,
            sign_exponent: st0.sign_exponent & 0x8000 | scaled as u16,
        }
    }
    else if n.is_nan() {
        st0 * st1
    }
    else if n.is_infinite() {
        st0 * if n > 0.0 { F80::POS_INFINITY } else { F80::ZERO }
    }
    else {
        // Three steps of 2^±16382 reach past the format from either end.
        let mut y = st0;
        let mut n = n.clamp(-3.0 * 16382.0, 3.0 * 16382.0) as i32;
        loop {
            let step = n.clamp(-16382, 16382);
            y = y * F80 {
                mantissa: 1 << 63,
                sign_exponent: (0x3FFF + step) as u16,
            };
            n -= step;
            if n == 0 {
                break;
            }
        }
        y
    };
    fpu_write_st_at(*fpu_stack_ptr as i32, y, X87_SITE_FSCALE);
}

pub unsafe fn fpu_fsin() {
    let st0 = fpu_get_st0();
    //if pow(-2.0, 63.0) < st0 && st0 < pow(2.0, 63.0) {
    fpu_write_st_at(*fpu_stack_ptr as i32, st0.sin(), X87_SITE_TRANSCENDENTAL);
    *fpu_status_word &= !FPU_C2;
    //}
    //else {
    //    *fpu_status_word |= FPU_C2;
    //}
}

pub unsafe fn fpu_fcos() {
    let st0 = fpu_get_st0();
    //if pow(-2.0, 63.0) < st0 && st0 < pow(2.0, 63.0) {
    fpu_write_st_at(*fpu_stack_ptr as i32, st0.cos(), X87_SITE_TRANSCENDENTAL);
    *fpu_status_word &= !FPU_C2;
    //}
    //else {
    //    *fpu_status_word |= FPU_C2;
    //}
}

pub unsafe fn fpu_fdecstp() {
    *fpu_stack_ptr = *fpu_stack_ptr - 1 & 7;
    *fpu_status_word &= !FPU_C1
}

pub unsafe fn fpu_fincstp() {
    *fpu_stack_ptr = *fpu_stack_ptr + 1 & 7;
    *fpu_status_word &= !FPU_C1
}

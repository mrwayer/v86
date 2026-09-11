extern "C" {
    fn extF80M_add(x: *const F80, y: *const F80, ptr: *mut F80);
    fn extF80M_sub(x: *const F80, y: *const F80, ptr: *mut F80);
    fn extF80M_mul(x: *const F80, y: *const F80, ptr: *mut F80);
    fn extF80M_div(x: *const F80, y: *const F80, ptr: *mut F80);
    //fn extF80M_rem(x: *const F80, y: *const F80, ptr: *mut F80);
    fn extF80M_sqrt(x: *const F80, ptr: *mut F80);

    fn extF80M_roundToInt(x: *const F80, rounding_mode: u8, raise_inexact: bool, dst: *mut F80);

    fn extF80M_eq(x: *const F80, y: *const F80) -> bool;
    //fn extF80M_eq_signaling(x: *const F80, y: *const F80) -> bool;

    //fn extF80M_le(x: *const F80, y: *const F80) -> bool;
    //fn extF80M_le_quiet(x: *const F80, y: *const F80) -> bool;
    fn extF80M_lt(x: *const F80, y: *const F80) -> bool;
    fn extF80M_lt_quiet(x: *const F80, y: *const F80) -> bool;

    fn extF80M_to_i32(src: *const F80, rounding_mode: u8, raise_inexact: bool) -> i32;
    fn extF80M_to_i64(src: *const F80, rounding_mode: u8, raise_inexact: bool) -> i64;
    fn i32_to_extF80M(src: i32, dst: *mut F80);
    fn i64_to_extF80M(src: i64, dst: *mut F80);

    fn f32_to_extF80M(src: i32, dst: *mut F80);
    fn f64_to_extF80M(src: u64, dst: *mut F80);
    fn extF80M_to_f32(src: *const F80) -> i32;
    fn extF80M_to_f64(src: *const F80) -> u64;

    static mut softfloat_roundingMode: u8;
    static mut extF80_roundingPrecision: u8;
    static mut softfloat_exceptionFlags: u8;
}

pub enum RoundingMode {
    NearEven,
    Trunc,
    Floor,
    Ceil,
}
pub enum Precision {
    P80,
    P64,
    P32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct F80 {
    pub mantissa: u64,
    pub sign_exponent: u16,
}

/// Whether arithmetic takes the double-precision shortcut.
///
/// The library computes in the 80-bit format the processor defines, in
/// software, and a program that keeps its numbers in x87 registers pays for
/// that on every operation. Nearly everything such a program does fits a
/// double: the shortcut converts both operands with bit arithmetic, computes
/// natively, and converts back, leaving to the library whatever a double
/// cannot hold -- a NaN, an infinity, a denormal, a result out of range --
/// and the exception flags those raise. What is given up is the eleven low
/// bits of the mantissa, which a program compiled for doubles never had, and
/// the inexact flag, which such a program never reads. Configuration index 5
/// of the compiler's knobs switches it off.
pub static mut FAST_F80: bool = true;

/// A normal or a zero as a double, rounded to nearest, ties to even; nothing
/// for what the shortcut leaves to the library.
#[inline]
fn to_f64_fast(x: &F80) -> Option<f64> {
    let sign = (x.sign_exponent as u64 >> 15) << 63;
    let exponent = (x.sign_exponent & 0x7FFF) as i32;
    if exponent == 0 && x.mantissa == 0 {
        return Some(f64::from_bits(sign));
    }
    if exponent == 0 || exponent == 0x7FFF || x.mantissa >> 63 == 0 {
        return None;
    }
    let biased = exponent - 0x3FFF + 1023;
    if biased < 1 || biased > 2046 {
        return None;
    }
    let fraction = x.mantissa & 0x7FFF_FFFF_FFFF_FFFF;
    let mut f52 = fraction >> 11;
    let rest = fraction & 0x7FF;
    if rest > 0x400 || (rest == 0x400 && f52 & 1 == 1) {
        f52 += 1;
    }
    let mut biased = biased as u64;
    if f52 >> 52 != 0 {
        f52 = 0;
        biased += 1;
        if biased > 2046 {
            return None;
        }
    }
    Some(f64::from_bits(sign | biased << 52 | f52))
}

/// A normal or a zero as a single, rounded once from the full mantissa.
#[inline]
fn to_f32_fast(x: &F80) -> Option<i32> {
    let sign = ((x.sign_exponent >> 15) as u32) << 31;
    let exponent = (x.sign_exponent & 0x7FFF) as i32;
    if exponent == 0 && x.mantissa == 0 {
        return Some(sign as i32);
    }
    if exponent == 0 || exponent == 0x7FFF || x.mantissa >> 63 == 0 {
        return None;
    }
    let biased = exponent - 0x3FFF + 127;
    if biased < 1 || biased > 254 {
        return None;
    }
    let fraction = x.mantissa & 0x7FFF_FFFF_FFFF_FFFF;
    let mut f23 = (fraction >> 40) as u32;
    let rest = fraction & 0xFF_FFFF_FFFF;
    if rest > 0x80_0000_0000 || (rest == 0x80_0000_0000 && f23 & 1 == 1) {
        f23 += 1;
    }
    let mut biased = biased as u32;
    if f23 >> 23 != 0 {
        f23 = 0;
        biased += 1;
        if biased > 254 {
            return None;
        }
    }
    Some((sign | biased << 23 | f23) as i32)
}

/// A double as the 80-bit format, exactly; nothing for a NaN, an infinity or a denormal.
#[inline]
fn of_f64_fast(v: f64) -> Option<F80> {
    let bits = v.to_bits();
    let sign = ((bits >> 63) as u16) << 15;
    let exponent = (bits >> 52 & 0x7FF) as i32;
    let fraction = bits & 0xF_FFFF_FFFF_FFFF;
    if exponent == 0 {
        return if fraction == 0 {
            Some(F80 {
                mantissa: 0,
                sign_exponent: sign,
            })
        }
        else {
            None
        };
    }
    if exponent == 0x7FF {
        return None;
    }
    Some(F80 {
        mantissa: 1 << 63 | fraction << 11,
        sign_exponent: sign | (exponent - 1023 + 0x3FFF) as u16,
    })
}

#[inline]
fn fast_binary(x: &F80, y: &F80, op: fn(f64, f64) -> f64) -> Option<F80> {
    if !unsafe { FAST_F80 } {
        return None;
    }
    of_f64_fast(op(to_f64_fast(x)?, to_f64_fast(y)?))
}

#[inline]
fn fast_pair(x: &F80, y: &F80) -> Option<(f64, f64)> {
    if !unsafe { FAST_F80 } {
        return None;
    }
    Some((to_f64_fast(x)?, to_f64_fast(y)?))
}

/// Rounds as the library's mode numbers say: 0 nearest-even, 1 towards zero,
/// 2 down, 3 up; nothing for a mode the shortcut does not know.
#[inline]
fn round_fast(d: f64, mode: u8) -> Option<f64> {
    Some(match mode {
        0 => {
            let truncated = d.trunc();
            if (d - truncated).abs() == 0.5 {
                if truncated % 2.0 == 0.0 { truncated } else { d.round() }
            }
            else {
                d.round()
            }
        },
        1 => d.trunc(),
        2 => d.floor(),
        3 => d.ceil(),
        _ => return None,
    })
}
impl F80 {
    pub const ZERO: F80 = F80 {
        mantissa: 0,
        sign_exponent: 0,
    };
    pub const ONE: F80 = F80 {
        mantissa: 0x8000000000000000,
        sign_exponent: 0x3FFF,
    };
    pub const LN_10: F80 = F80 {
        mantissa: 0x935D8DDDAAA8B000,
        sign_exponent: 0x4000,
    };
    pub const LN_2: F80 = F80 {
        mantissa: 0xB17217F7D1CF7800,
        sign_exponent: 0x3FFE,
    };
    pub const PI: F80 = F80 {
        mantissa: 0xC90FDAA22168C000,
        sign_exponent: 0x4000,
    };
    pub const LOG2_E: F80 = F80 {
        mantissa: 0xB8AA3B295C17F000,
        sign_exponent: 0x3FFF,
    };
    pub const INDEFINITE_NAN: F80 = F80 {
        mantissa: 0xC000000000000000,
        sign_exponent: 0x7FFF,
    };
    pub const POS_INFINITY: F80 = F80 {
        mantissa: 0x8000000000000000,
        sign_exponent: 0x7FFF,
    };
    pub const NEG_INFINITY: F80 = F80 {
        mantissa: 0x8000000000000000,
        sign_exponent: 0xFFFF,
    };

    pub fn sign(&self) -> bool { (self.sign_exponent >> 15) == 1 }
    pub fn exponent(&self) -> i16 { (self.sign_exponent as i16 & 0x7FFF) - 0x3FFF }

    pub fn of_i32(src: i32) -> F80 {
        if unsafe { FAST_F80 } {
            if let Some(x) = of_f64_fast(src as f64) {
                return x;
            }
        }
        let mut x = F80::ZERO;
        unsafe { i32_to_extF80M(src, &mut x) };
        x
    }
    pub fn of_i64(src: i64) -> F80 {
        let mut x = F80::ZERO;
        unsafe { i64_to_extF80M(src, &mut x) };
        x
    }

    pub fn of_f32(src: i32) -> F80 {
        if unsafe { FAST_F80 } {
            if let Some(x) = of_f64_fast(f32::from_bits(src as u32) as f64) {
                return x;
            }
        }
        let mut x = F80::ZERO;
        unsafe { f32_to_extF80M(src, &mut x) };
        x
    }

    pub fn of_f64(src: u64) -> F80 {
        unsafe { crate::cpu::cpu::note_f80_conversion() };
        if unsafe { FAST_F80 } {
            if let Some(x) = of_f64_fast(f64::from_bits(src)) {
                return x;
            }
        }
        let mut x = F80::ZERO;
        unsafe { f64_to_extF80M(src, &mut x) };
        x
    }
    fn of_f64x(src: f64) -> F80 { F80::of_f64(f64::to_bits(src)) }

    pub fn to_f32(&self) -> i32 {
        if unsafe { FAST_F80 } {
            if let Some(x) = to_f32_fast(self) {
                return x;
            }
        }
        unsafe { extF80M_to_f32(self) }
    }
    pub fn to_f64(&self) -> u64 {
        unsafe { crate::cpu::cpu::note_f80_conversion() };
        if unsafe { FAST_F80 } {
            if let Some(x) = to_f64_fast(self) {
                return x.to_bits();
            }
        }
        unsafe { extF80M_to_f64(self) }
    }
    fn to_f64x(&self) -> f64 { f64::from_bits(self.to_f64()) }

    fn to_i32_fast(&self, mode: u8) -> Option<i32> {
        if !unsafe { FAST_F80 } {
            return None;
        }
        let rounded = round_fast(to_f64_fast(self)?, mode)?;
        if rounded >= -2147483648.0 && rounded <= 2147483647.0 { Some(rounded as i32) } else { None }
    }
    fn to_i64_fast(&self, mode: u8) -> Option<i64> {
        if !unsafe { FAST_F80 } {
            return None;
        }
        let rounded = round_fast(to_f64_fast(self)?, mode)?;
        if rounded >= -9223372036854775808.0 && rounded < 9223372036854775808.0 {
            Some(rounded as i64)
        }
        else {
            None
        }
    }
    pub fn to_i32(&self) -> i32 {
        let mode = unsafe { softfloat_roundingMode };
        self.to_i32_fast(mode).unwrap_or_else(|| unsafe { extF80M_to_i32(self, mode, false) })
    }
    pub fn to_i64(&self) -> i64 {
        let mode = unsafe { softfloat_roundingMode };
        self.to_i64_fast(mode).unwrap_or_else(|| unsafe { extF80M_to_i64(self, mode, false) })
    }

    pub fn truncate_to_i32(&self) -> i32 {
        self.to_i32_fast(1).unwrap_or_else(|| unsafe { extF80M_to_i32(self, 1, false) })
    }
    pub fn truncate_to_i64(&self) -> i64 {
        self.to_i64_fast(1).unwrap_or_else(|| unsafe { extF80M_to_i64(self, 1, false) })
    }

    pub fn cos(self) -> F80 { F80::of_f64x(self.to_f64x().cos()) }
    pub fn sin(self) -> F80 { F80::of_f64x(self.to_f64x().sin()) }
    pub fn tan(self) -> F80 { F80::of_f64x(self.to_f64x().tan()) }
    pub fn atan(self) -> F80 { F80::of_f64x(self.to_f64x().atan()) }
    pub fn atan2(self, other: F80) -> F80 { F80::of_f64x(self.to_f64x().atan2(other.to_f64x())) }

    pub fn log2(self) -> F80 { F80::of_f64x(self.to_f64x().log2()) }
    pub fn ln(self) -> F80 { F80::of_f64x(self.to_f64x().ln()) }

    pub fn abs(self) -> F80 {
        F80 {
            mantissa: self.mantissa,
            sign_exponent: self.sign_exponent & !0x8000,
        }
    }
    pub fn two_pow(self) -> F80 { F80::of_f64x(2.0f64.powf(self.to_f64x())) }
    pub fn round(self) -> F80 {
        let mut result = F80::ZERO;
        unsafe { extF80M_roundToInt(&self, softfloat_roundingMode, false, &mut result) };
        result
    }
    pub fn trunc(self) -> F80 {
        let mut result = F80::ZERO;
        unsafe { extF80M_roundToInt(&self, 1, false, &mut result) };
        result
    }

    pub fn sqrt(self) -> F80 {
        let mut result = F80::ZERO;
        unsafe { extF80M_sqrt(&self, &mut result) };
        result
    }

    pub fn is_finite(self) -> bool {
        // TODO: Can probably be done more efficiently
        self != F80::POS_INFINITY && self != F80::NEG_INFINITY
    }
    pub fn is_nan(self) -> bool {
        // TODO: Can probably be done more efficiently
        self != self
    }

    pub fn set_rounding_mode(mode: RoundingMode) {
        unsafe {
            softfloat_roundingMode = match mode {
                RoundingMode::NearEven => 0,
                RoundingMode::Trunc => 1,
                RoundingMode::Floor => 2,
                RoundingMode::Ceil => 3,
            }
        };
    }
    pub fn set_precision(precision: Precision) {
        unsafe {
            extF80_roundingPrecision = match precision {
                Precision::P80 => 80,
                Precision::P64 => 64,
                Precision::P32 => 32,
            }
        };
    }

    pub fn get_exception_flags() -> u8 {
        let f = unsafe { softfloat_exceptionFlags };
        // translate softfloat's flags to x87 status flags
        f >> 4 & 1 | f >> 1 & 4 | f << 3 & 16
    }
    pub fn clear_exception_flags() { unsafe { softfloat_exceptionFlags = 0 } }

    pub fn partial_cmp_quiet(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if let Some((x, y)) = fast_pair(self, other) {
            return x.partial_cmp(&y);
        }
        // TODO: Can probably be done more efficiently
        if unsafe { extF80M_lt_quiet(self, other) } {
            Some(std::cmp::Ordering::Less)
        }
        else if unsafe { extF80M_lt_quiet(other, self) } {
            Some(std::cmp::Ordering::Greater)
        }
        else if self == other {
            Some(std::cmp::Ordering::Equal)
        }
        else {
            None
        }
    }
}

impl std::ops::Add for F80 {
    type Output = F80;
    fn add(self, other: Self) -> Self {
        if let Some(result) = fast_binary(&self, &other, |x, y| x + y) {
            return result;
        }
        let mut result = F80::ZERO;
        unsafe { extF80M_add(&self, &other, &mut result) };
        result
    }
}
impl std::ops::Sub for F80 {
    type Output = F80;
    fn sub(self, other: Self) -> Self {
        if let Some(result) = fast_binary(&self, &other, |x, y| x - y) {
            return result;
        }
        let mut result = F80::ZERO;
        unsafe { extF80M_sub(&self, &other, &mut result) };
        result
    }
}
impl std::ops::Neg for F80 {
    type Output = F80;
    fn neg(self) -> Self {
        let mut result = self;
        result.sign_exponent ^= 1 << 15;
        result
    }
}
impl std::ops::Mul for F80 {
    type Output = F80;
    fn mul(self, other: Self) -> Self {
        if let Some(result) = fast_binary(&self, &other, |x, y| x * y) {
            return result;
        }
        let mut result = F80::ZERO;
        unsafe { extF80M_mul(&self, &other, &mut result) };
        result
    }
}
impl std::ops::Div for F80 {
    type Output = F80;
    fn div(self, other: Self) -> Self {
        if let Some(result) = fast_binary(&self, &other, |x, y| x / y) {
            return result;
        }
        let mut result = F80::ZERO;
        unsafe { extF80M_div(&self, &other, &mut result) };
        result
    }
}
impl std::ops::Rem for F80 {
    type Output = F80;
    fn rem(self, other: Self) -> Self {
        let quot = (self / other).trunc();
        self - quot * other
        // Uses round-to-nearest instead of truncation
        //let mut result = F80::ZERO;
        //unsafe {
        //    extF80M_rem(&self, &other, &mut result)
        //};
        //result
    }
}

impl PartialEq for F80 {
    fn eq(&self, other: &Self) -> bool {
        if let Some((x, y)) = fast_pair(self, other) {
            return x == y;
        }
        unsafe { extF80M_eq(self, other) }
    }
}
impl PartialOrd for F80 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if let Some((x, y)) = fast_pair(self, other) {
            return x.partial_cmp(&y);
        }
        // TODO: Can probably be done more efficiently
        if unsafe { extF80M_lt(self, other) } {
            Some(std::cmp::Ordering::Less)
        }
        else if unsafe { extF80M_lt(other, self) } {
            Some(std::cmp::Ordering::Greater)
        }
        else if self == other {
            Some(std::cmp::Ordering::Equal)
        }
        else {
            None
        }
    }
}

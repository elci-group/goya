//! ARMv4T arithmetic and barrel-shifter semantics. No instruction execution,
//! processor modes, memory access, or cycle accounting is implied by this API.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Flags {
    pub n: bool,
    pub z: bool,
    pub c: bool,
    pub v: bool,
}

impl Flags {
    /// Evaluate a four-bit ARM condition field. NV (15) never executes on
    /// ARMv4T. Invalid wider values are rejected instead of being truncated.
    pub fn condition(self, condition: u8) -> Option<bool> {
        Some(match condition {
            0 => self.z,
            1 => !self.z,
            2 => self.c,
            3 => !self.c,
            4 => self.n,
            5 => !self.n,
            6 => self.v,
            7 => !self.v,
            8 => self.c && !self.z,
            9 => !self.c || self.z,
            10 => self.n == self.v,
            11 => self.n != self.v,
            12 => !self.z && self.n == self.v,
            13 => self.z || self.n != self.v,
            14 => true,
            15 => false,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Arithmetic {
    pub value: u32,
    pub flags: Flags,
}

/// Add two words and an incoming carry, with independent signed overflow
/// and unsigned carry. Subtraction uses `add_with_carry(a, !b, true)`;
/// SBC uses the incoming CPSR C directly (C means no borrow).
pub fn add_with_carry(a: u32, b: u32, carry: bool) -> Arithmetic {
    let unsigned = u64::from(a) + u64::from(b) + u64::from(carry);
    let signed = i64::from(a as i32) + i64::from(b as i32) + i64::from(carry);
    let value = unsigned as u32;
    Arithmetic {
        value,
        flags: Flags {
            n: value & 0x8000_0000 != 0,
            z: value == 0,
            c: unsigned > u64::from(u32::MAX),
            v: signed < i64::from(i32::MIN) || signed > i64::from(i32::MAX),
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shift {
    Lsl,
    Lsr,
    Asr,
    Ror,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Shifted {
    pub value: u32,
    pub carry: bool,
}

/// Register-controlled shift uses only the low byte of Rs, without reducing
/// large shifts modulo 32 (except for rotation). A zero amount preserves C.
pub fn shift_register(value: u32, kind: Shift, rs: u32, carry: bool) -> Shifted {
    let amount = rs & 255;
    shift(value, kind, amount, carry)
}

/// Decode an immediate shifter's five-bit amount. Encoded LSR/ASR zero means
/// 32, while encoded ROR zero means RRX. Invalid fields are rejected.
pub fn shift_immediate(value: u32, kind: Shift, encoded: u8, carry: bool) -> Option<Shifted> {
    if encoded > 31 {
        return None;
    }
    if encoded == 0 {
        match kind {
            Shift::Lsr | Shift::Asr => return Some(shift(value, kind, 32, carry)),
            Shift::Ror => {
                return Some(Shifted {
                    value: (u32::from(carry) << 31) | (value >> 1),
                    carry: value & 1 != 0,
                })
            }
            Shift::Lsl => {}
        }
    }
    Some(shift(value, kind, u32::from(encoded), carry))
}

fn shift(value: u32, kind: Shift, amount: u32, carry: bool) -> Shifted {
    if amount == 0 {
        return Shifted { value, carry };
    }
    let (value, carry) = match kind {
        Shift::Lsl if amount < 32 => (value << amount, value & (1 << (32 - amount)) != 0),
        Shift::Lsl if amount == 32 => (0, value & 1 != 0),
        Shift::Lsl => (0, false),
        Shift::Lsr if amount < 32 => (value >> amount, value & (1 << (amount - 1)) != 0),
        Shift::Lsr if amount == 32 => (0, value >> 31 != 0),
        Shift::Lsr => (0, false),
        Shift::Asr if amount < 32 => (
            ((value as i32) >> amount) as u32,
            value & (1 << (amount - 1)) != 0,
        ),
        Shift::Asr => (
            if value >> 31 != 0 { u32::MAX } else { 0 },
            value >> 31 != 0,
        ),
        Shift::Ror => {
            let rotated = value.rotate_right(amount);
            (rotated, rotated >> 31 != 0)
        }
    };
    Shifted { value, carry }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_carry_overflow_and_borrow_are_independent() {
        let sum = add_with_carry(u32::MAX, 0, true);
        assert_eq!(sum.value, 0);
        assert_eq!(
            sum.flags,
            Flags {
                n: false,
                z: true,
                c: true,
                v: false
            }
        );
        let sum = add_with_carry(0x7fff_ffff, 0, true);
        assert!(sum.flags.n && sum.flags.v && !sum.flags.c);
        let difference = add_with_carry(0, !1, true);
        assert_eq!(difference.value, u32::MAX);
        assert!(!difference.flags.c && !difference.flags.v);
        let difference = add_with_carry(0x8000_0000, !1, true);
        assert_eq!(difference.value, 0x7fff_ffff);
        assert!(difference.flags.c && difference.flags.v);
    }

    #[test]
    fn register_shifts_match_single_bit_steps_for_all_low_byte_amounts() {
        for value in [0, 1, 0x8000_0000, 0x1234_abcd, u32::MAX] {
            for kind in [Shift::Lsl, Shift::Lsr, Shift::Asr, Shift::Ror] {
                for carry in [false, true] {
                    let mut expected = Shifted { value, carry };
                    for amount in 0..256 {
                        assert_eq!(shift_register(value, kind, amount, carry), expected);
                        assert_eq!(shift_register(value, kind, amount + 256, carry), expected);
                        expected = match kind {
                            Shift::Lsl => Shifted {
                                value: expected.value << 1,
                                carry: expected.value >> 31 != 0,
                            },
                            Shift::Lsr => Shifted {
                                value: expected.value >> 1,
                                carry: expected.value & 1 != 0,
                            },
                            Shift::Asr => Shifted {
                                value: ((expected.value as i32) >> 1) as u32,
                                carry: expected.value & 1 != 0,
                            },
                            Shift::Ror => Shifted {
                                value: expected.value.rotate_right(1),
                                carry: expected.value & 1 != 0,
                            },
                        };
                    }
                }
            }
        }
    }

    #[test]
    fn encoded_zero_shifts_and_condition_pairs() {
        assert_eq!(
            shift_immediate(1, Shift::Ror, 0, true),
            Some(Shifted {
                value: 0x8000_0000,
                carry: true
            })
        );
        assert_eq!(
            shift_immediate(0x8000_0000, Shift::Lsr, 0, false),
            Some(Shifted {
                value: 0,
                carry: true
            })
        );
        assert!(shift_immediate(0, Shift::Lsl, 32, false).is_none());
        for bits in 0..16 {
            let flags = Flags {
                n: bits & 8 != 0,
                z: bits & 4 != 0,
                c: bits & 2 != 0,
                v: bits & 1 != 0,
            };
            for condition in (0..14).step_by(2) {
                assert_ne!(flags.condition(condition), flags.condition(condition + 1));
            }
            assert_eq!(flags.condition(14), Some(true));
            assert_eq!(flags.condition(15), Some(false));
            assert_eq!(flags.condition(16), None);
        }
    }
}

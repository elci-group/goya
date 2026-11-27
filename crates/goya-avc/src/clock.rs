//! Fixed-point media clock (ADR-AVC-001 F5, decision 5).
//!
//! AVR found that a floating-point spatial pipeline forces a
//! reproducibility split between derivation and rendering. The media clock
//! is created new, so the lesson is applied at design time: every timeline
//! position is an integer tick count over a rational timescale. NTSC-style
//! rates are representable exactly. Floating-point timestamps never enter
//! the contract: there is deliberately no `From<f64>` conversion, and the
//! contract boundary rejects one implicitly by not accepting it.

/// A position on the media clock: `ticks / ticks_per_second` seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MediaClock {
    ticks: u64,
    ticks_per_second: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    /// A timescale or denominator of zero is undefined.
    ZeroTimescale,
    /// Converting a rational to this clock's timescale overflowed.
    Overflow,
}

/// Divide `numerator / denominator` with round-half-up on exact halves
/// (the rule is fixed so every host produces identical ticks).
const fn div_round_half_up(numerator: u128, denominator: u128) -> u128 {
    (numerator + denominator / 2) / denominator
}

impl MediaClock {
    /// A clock position of `ticks` ticks over `ticks_per_second`.
    pub const fn new(ticks: u64, ticks_per_second: u32) -> Result<Self, ClockError> {
        if ticks_per_second == 0 {
            return Err(ClockError::ZeroTimescale);
        }
        Ok(Self {
            ticks,
            ticks_per_second,
        })
    }

    /// The position `numerator / denominator` seconds, snapped to this
    /// clock's timescale.
    pub fn from_rational(
        numerator: u64,
        denominator: u32,
        ticks_per_second: u32,
    ) -> Result<Self, ClockError> {
        if denominator == 0 {
            return Err(ClockError::ZeroTimescale);
        }
        let ticks = div_round_half_up(
            u128::from(numerator) * u128::from(ticks_per_second),
            u128::from(denominator),
        );
        let ticks = u64::try_from(ticks).map_err(|_| ClockError::Overflow)?;
        Self::new(ticks, ticks_per_second)
    }

    pub const fn ticks(&self) -> u64 {
        self.ticks
    }

    pub const fn ticks_per_second(&self) -> u32 {
        self.ticks_per_second
    }

    /// Advance by `ticks` ticks, rejecting overflow.
    pub fn checked_add_ticks(&self, ticks: u64) -> Result<Self, ClockError> {
        self.ticks
            .checked_add(ticks)
            .map(|t| Self {
                ticks: t,
                ticks_per_second: self.ticks_per_second,
            })
            .ok_or(ClockError::Overflow)
    }

    /// Duration of one frame at `numerator / denominator` Hz, snapped to
    /// `ticks_per_second` (e.g. 30000/1001 Hz for NTSC 29.97).
    pub fn frame_duration(
        numerator: u32,
        denominator: u32,
        ticks_per_second: u32,
    ) -> Result<Self, ClockError> {
        if numerator == 0 {
            return Err(ClockError::ZeroTimescale);
        }
        // ticks = ticks_per_second * denominator / numerator
        let ticks = div_round_half_up(
            u128::from(ticks_per_second) * u128::from(denominator),
            u128::from(numerator),
        );
        let ticks = u64::try_from(ticks).map_err(|_| ClockError::Overflow)?;
        Self::new(ticks, ticks_per_second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_positions_snap_deterministically() {
        let a = MediaClock::from_rational(1, 3, 90).unwrap();
        assert_eq!(a.ticks(), 30);
        // 1.5 ticks rounds up to 2.
        let b = MediaClock::from_rational(3, 4, 2).unwrap();
        assert_eq!(b.ticks(), 2);
    }

    #[test]
    fn ntsc_frame_rate_is_representable() {
        // 30000/1001 Hz video: one frame is 1001 ticks at a 30000 tps clock.
        let frame = MediaClock::frame_duration(30_000, 1_001, 30_000).unwrap();
        assert_eq!(frame.ticks(), 1_001);
        assert_eq!(frame.ticks_per_second(), 30_000);
        // Two consecutive frames land on distinct exact positions.
        let next = frame.checked_add_ticks(frame.ticks()).unwrap();
        assert_eq!(next.ticks(), 2_002);
    }

    #[test]
    fn zero_timescale_is_rejected() {
        assert_eq!(MediaClock::new(1, 0), Err(ClockError::ZeroTimescale));
        assert_eq!(
            MediaClock::from_rational(1, 0, 30),
            Err(ClockError::ZeroTimescale)
        );
        assert_eq!(
            MediaClock::frame_duration(0, 30, 30),
            Err(ClockError::ZeroTimescale)
        );
    }

    #[test]
    fn ordering_and_checked_addition() {
        let a = MediaClock::new(10, 30).unwrap();
        let b = MediaClock::new(11, 30).unwrap();
        assert!(a < b);
        assert_eq!(a.checked_add_ticks(1).unwrap(), b);
        assert_eq!(a.checked_add_ticks(u64::MAX), Err(ClockError::Overflow));
    }
}

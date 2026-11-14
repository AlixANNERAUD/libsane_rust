//! `SANE_Fixed`: signed fixed-point numbers with 16 fractional bits.

use std::fmt;

use sane_sys as sys;

/// A SANE fixed-point number (`SANE_Fixed`): 16 integer bits, 16 fractional
/// bits, from about -32768 to 32767.99998 in steps of 1/65536.
///
/// ```
/// use libsane::Fixed;
///
/// let width = Fixed::from_f64(215.9);
/// assert!((width.to_f64() - 215.9).abs() < 1.0 / 65536.0);
/// assert_eq!(Fixed::from(3).to_f64(), 3.0);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(sys::SANE_Fixed);

impl Fixed {
    /// The smallest value.
    pub const MIN: Self = Self(sys::SANE_Fixed::MIN);
    /// The largest value.
    pub const MAX: Self = Self(sys::SANE_Fixed::MAX);
    /// The distance between two consecutive values (1/65536).
    pub const EPSILON: Self = Self(1);

    /// The fixed-point number with this raw representation.
    pub const fn from_raw(raw: sys::SANE_Fixed) -> Self {
        Self(raw)
    }

    /// The raw representation (the value multiplied by 65536).
    pub const fn to_raw(self) -> sys::SANE_Fixed {
        self.0
    }

    /// Converts a floating-point number, like the C macro `SANE_FIX`:
    /// truncates toward zero and saturates when out of range (NaN becomes 0).
    pub fn from_f64(value: f64) -> Self {
        Self(sys::SANE_FIX(value))
    }

    /// Converts to a floating-point number (exact), like `SANE_UNFIX`.
    pub fn to_f64(self) -> f64 {
        sys::SANE_UNFIX(self.0)
    }
}

impl From<i16> for Fixed {
    fn from(value: i16) -> Self {
        Self(sys::SANE_Fixed::from(value) << sys::SANE_FIXED_SCALE_SHIFT)
    }
}

impl From<Fixed> for f64 {
    fn from(value: Fixed) -> Self {
        value.to_f64()
    }
}

impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed({})", self.to_f64())
    }
}

impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_f64(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(Fixed::from(1).to_raw(), 65536);
        assert_eq!(Fixed::from(-1).to_raw(), -65536);
        assert_eq!(Fixed::from_f64(0.5).to_raw(), 32768);
        assert_eq!(Fixed::from_f64(-0.5).to_f64(), -0.5);
        assert_eq!(Fixed::from_f64(1e9), Fixed::MAX);
        assert_eq!(Fixed::from_f64(-1e9), Fixed::MIN);
        assert_eq!(Fixed::from_f64(f64::NAN), Fixed::default());
        assert_eq!(f64::from(Fixed::from_raw(3 << 15)), 1.5);
        assert_eq!(Fixed::from(i16::MIN), Fixed::MIN);
        assert_eq!(Fixed::from_f64(2.25).to_string(), "2.25");
    }

    #[test]
    fn ordering_follows_the_value() {
        assert!(Fixed::from(-2) < Fixed::from_f64(-1.5));
        assert!(Fixed::from_f64(0.1) < Fixed::from_f64(0.2));
    }
}

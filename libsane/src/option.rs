//! Device options: descriptors, values and constraints.

use sane_sys as sys;

use crate::Fixed;
use crate::error::{Error, Result};

/// The type of the value of an option (`SANE_Value_Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValueType {
    /// A boolean.
    Bool,
    /// One or several integers.
    Int,
    /// One or several fixed-point numbers ([`Fixed`]).
    Fixed,
    /// A string.
    String,
    /// No value: setting the option performs an action.
    Button,
    /// No value: starts a group of options, for presentation only.
    Group,
    /// A type this crate does not know.
    Unknown(u32),
}

impl ValueType {
    pub(crate) fn from_raw(raw: sys::SANE_Value_Type) -> Self {
        match raw {
            sys::SANE_TYPE_BOOL => Self::Bool,
            sys::SANE_TYPE_INT => Self::Int,
            sys::SANE_TYPE_FIXED => Self::Fixed,
            sys::SANE_TYPE_STRING => Self::String,
            sys::SANE_TYPE_BUTTON => Self::Button,
            sys::SANE_TYPE_GROUP => Self::Group,
            other => Self::Unknown(other),
        }
    }

    /// Whether values of this type are stored as words (bool, int, fixed).
    fn is_word(self) -> bool {
        matches!(self, Self::Bool | Self::Int | Self::Fixed)
    }
}

/// The physical unit of an option (`SANE_Unit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Unit {
    /// No unit.
    None,
    /// Pixels.
    Pixel,
    /// Bits.
    Bit,
    /// Millimeters.
    Mm,
    /// Dots per inch.
    Dpi,
    /// A percentage.
    Percent,
    /// Microseconds.
    Microsecond,
    /// A unit this crate does not know.
    Unknown(u32),
}

impl Unit {
    pub(crate) fn from_raw(raw: sys::SANE_Unit) -> Self {
        match raw {
            sys::SANE_UNIT_NONE => Self::None,
            sys::SANE_UNIT_PIXEL => Self::Pixel,
            sys::SANE_UNIT_BIT => Self::Bit,
            sys::SANE_UNIT_MM => Self::Mm,
            sys::SANE_UNIT_DPI => Self::Dpi,
            sys::SANE_UNIT_PERCENT => Self::Percent,
            sys::SANE_UNIT_MICROSECOND => Self::Microsecond,
            other => Self::Unknown(other),
        }
    }
}

/// The capabilities of an option (`SANE_CAP_*`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Capabilities(u32);

impl Capabilities {
    /// `SANE_CAP_SOFT_SELECT`: the value can be set by software.
    pub const SOFT_SELECT: Self = Self(sys::SANE_CAP_SOFT_SELECT);
    /// `SANE_CAP_HARD_SELECT`: the value is set by a switch or button on the
    /// device.
    pub const HARD_SELECT: Self = Self(sys::SANE_CAP_HARD_SELECT);
    /// `SANE_CAP_SOFT_DETECT`: the value can be read by software.
    pub const SOFT_DETECT: Self = Self(sys::SANE_CAP_SOFT_DETECT);
    /// `SANE_CAP_EMULATED`: the option is emulated by the backend.
    pub const EMULATED: Self = Self(sys::SANE_CAP_EMULATED);
    /// `SANE_CAP_AUTOMATIC`: the backend can choose the value itself
    /// ([`Device::set_auto`](crate::Device::set_auto)).
    pub const AUTOMATIC: Self = Self(sys::SANE_CAP_AUTOMATIC);
    /// `SANE_CAP_INACTIVE`: the option is currently inactive.
    pub const INACTIVE: Self = Self(sys::SANE_CAP_INACTIVE);
    /// `SANE_CAP_ADVANCED`: the option is for advanced users.
    pub const ADVANCED: Self = Self(sys::SANE_CAP_ADVANCED);

    /// The capabilities with these `SANE_CAP_*` bits.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// The `SANE_CAP_*` bits.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Whether every capability of `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the option is active (`SANE_OPTION_IS_ACTIVE`).
    pub const fn is_active(self) -> bool {
        !self.contains(Self::INACTIVE)
    }

    /// Whether the option can be set by software (`SANE_OPTION_IS_SETTABLE`).
    pub const fn is_settable(self) -> bool {
        self.contains(Self::SOFT_SELECT)
    }

    /// Whether the value can be read by software.
    pub const fn is_readable(self) -> bool {
        self.contains(Self::SOFT_DETECT)
    }
}

impl std::ops::BitOr for Capabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::fmt::Debug for Capabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const NAMES: [(Capabilities, &str); 7] = [
            (Capabilities::SOFT_SELECT, "SOFT_SELECT"),
            (Capabilities::HARD_SELECT, "HARD_SELECT"),
            (Capabilities::SOFT_DETECT, "SOFT_DETECT"),
            (Capabilities::EMULATED, "EMULATED"),
            (Capabilities::AUTOMATIC, "AUTOMATIC"),
            (Capabilities::INACTIVE, "INACTIVE"),
            (Capabilities::ADVANCED, "ADVANCED"),
        ];
        let mut set = f.debug_set();
        let mut known = 0;
        for (capability, name) in NAMES {
            if self.contains(capability) {
                set.entry(&format_args!("{name}"));
            }
            known |= capability.0;
        }
        if self.0 & !known != 0 {
            set.entry(&format_args!("{:#x}", self.0 & !known));
        }
        set.finish()
    }
}

/// An inclusive range of values with an optional quantization step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range<T> {
    /// The smallest value.
    pub min: T,
    /// The largest value.
    pub max: T,
    /// The step between values (`0` when any value is allowed).
    pub quant: T,
}

/// The values an option accepts (`SANE_Constraint_Type`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Constraint {
    /// Any value of the type.
    None,
    /// An integer range (`SANE_CONSTRAINT_RANGE` on an int option).
    IntRange(Range<i32>),
    /// A fixed-point range (`SANE_CONSTRAINT_RANGE` on a fixed option).
    FixedRange(Range<Fixed>),
    /// A list of integers (`SANE_CONSTRAINT_WORD_LIST` on an int option).
    IntList(Vec<i32>),
    /// A list of fixed-point numbers (`SANE_CONSTRAINT_WORD_LIST` on a fixed
    /// option).
    FixedList(Vec<Fixed>),
    /// A list of strings (`SANE_CONSTRAINT_STRING_LIST`).
    StringList(Vec<String>),
}

/// The description of an option of a device (`SANE_Option_Descriptor`).
///
/// This is a copy: it stays valid after the device reloads its options, but
/// may then be outdated. [`Device::options`](crate::Device::options) always
/// returns the current descriptors.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct OptionDescriptor {
    /// The index of the option on its device.
    pub index: usize,
    /// The unique name of the option, used on command lines (empty for
    /// groups).
    pub name: String,
    /// The title of the option, a single line.
    pub title: String,
    /// The description of the option, possibly several lines.
    pub description: String,
    /// The type of the value.
    pub value_type: ValueType,
    /// The physical unit of the value.
    pub unit: Unit,
    /// The size of the value in bytes: for words (bool, int, fixed), four
    /// bytes per element; for strings, the maximum length including the
    /// terminating NUL.
    pub size: usize,
    /// The capabilities of the option.
    pub capabilities: Capabilities,
    /// The values the option accepts.
    pub constraint: Constraint,
}

impl OptionDescriptor {
    /// Whether the option is active.
    pub fn is_active(&self) -> bool {
        self.capabilities.is_active()
    }

    /// Whether the option can be set by software.
    pub fn is_settable(&self) -> bool {
        self.capabilities.is_settable()
    }

    /// Whether the value can be read by software.
    pub fn is_readable(&self) -> bool {
        self.capabilities.is_readable()
    }

    /// The number of elements of a bool, int or fixed option (1 unless the
    /// option is an array), 0 for other types.
    pub fn len(&self) -> usize {
        if self.value_type.is_word() {
            (self.size / WORD_SIZE).max(1)
        } else {
            0
        }
    }

    /// Whether the option has no element (see [`len`](Self::len)).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A name for error messages: the name, or the index for unnamed options.
    pub(crate) fn label(&self) -> String {
        if self.name.is_empty() {
            format!("#{}", self.index)
        } else {
            self.name.clone()
        }
    }
}

/// The value of an option.
///
/// [`Device::set_value`](crate::Device::set_value) takes anything that
/// converts into a `Value`: `bool`, `i32`, `f64` and [`Fixed`], `&str` and
/// `String`, `Vec<i32>` and `Vec<Fixed>`. An integer is accepted for a fixed
/// option and converted.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// The value of a bool option.
    Bool(bool),
    /// The value of an int option with one element.
    Int(i32),
    /// The value of a fixed option with one element.
    Fixed(Fixed),
    /// The value of a string option.
    String(String),
    /// The value of an int option with several elements.
    IntArray(Vec<i32>),
    /// The value of a fixed option with several elements.
    FixedArray(Vec<Fixed>),
}

impl Value {
    /// The boolean, for a bool value.
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Self::Bool(value) => Some(value),
            _ => None,
        }
    }

    /// The integer, for an int value with one element.
    pub fn as_int(&self) -> Option<i32> {
        match *self {
            Self::Int(value) => Some(value),
            _ => None,
        }
    }

    /// The number, for an int or fixed value with one element.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::Int(value) => Some(value.into()),
            Self::Fixed(value) => Some(value.to_f64()),
            _ => None,
        }
    }

    /// The string, for a string value.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}

impl From<Fixed> for Value {
    fn from(value: Fixed) -> Self {
        Self::Fixed(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Fixed(Fixed::from_f64(value))
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<Vec<i32>> for Value {
    fn from(value: Vec<i32>) -> Self {
        Self::IntArray(value)
    }
}

impl From<Vec<Fixed>> for Value {
    fn from(value: Vec<Fixed>) -> Self {
        Self::FixedArray(value)
    }
}

/// What changed after setting an option (`SANE_INFO_*`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SetInfo(u32);

impl SetInfo {
    pub(crate) fn from_raw(raw: sys::SANE_Int) -> Self {
        Self(raw as u32)
    }

    /// `SANE_INFO_INEXACT`: the backend rounded the value; read it back for
    /// the value used.
    pub fn inexact(self) -> bool {
        self.0 & sys::SANE_INFO_INEXACT != 0
    }

    /// `SANE_INFO_RELOAD_OPTIONS`: other options changed (activity,
    /// constraints...). The [`Device`](crate::Device) already reloaded its
    /// descriptors; copies made before are outdated.
    pub fn reload_options(self) -> bool {
        self.0 & sys::SANE_INFO_RELOAD_OPTIONS != 0
    }

    /// `SANE_INFO_RELOAD_PARAMS`: the scan parameters changed.
    pub fn reload_params(self) -> bool {
        self.0 & sys::SANE_INFO_RELOAD_PARAMS != 0
    }

    /// The `SANE_INFO_*` bits.
    pub fn bits(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOrAssign for SetInfo {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::fmt::Debug for SetInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetInfo")
            .field("inexact", &self.inexact())
            .field("reload_options", &self.reload_options())
            .field("reload_params", &self.reload_params())
            .finish()
    }
}

/// Identifies an option of a device: its index (`usize`), its name (`&str`,
/// `String`), or its descriptor.
pub trait OptionId: private::Sealed {
    #[doc(hidden)]
    fn find_in<'a>(&self, options: &'a [OptionDescriptor]) -> Option<&'a OptionDescriptor>;
    #[doc(hidden)]
    fn label(&self) -> String;
}

mod private {
    pub trait Sealed {}
    impl Sealed for usize {}
    impl Sealed for str {}
    impl Sealed for String {}
    impl Sealed for super::OptionDescriptor {}
    impl<T: Sealed + ?Sized> Sealed for &T {}
}

impl OptionId for usize {
    fn find_in<'a>(&self, options: &'a [OptionDescriptor]) -> Option<&'a OptionDescriptor> {
        options.iter().find(|option| option.index == *self)
    }

    fn label(&self) -> String {
        format!("#{self}")
    }
}

impl OptionId for str {
    fn find_in<'a>(&self, options: &'a [OptionDescriptor]) -> Option<&'a OptionDescriptor> {
        // Groups have no name: an empty name finds nothing.
        options
            .iter()
            .find(|option| !self.is_empty() && option.name == self)
    }

    fn label(&self) -> String {
        self.to_owned()
    }
}

impl OptionId for String {
    fn find_in<'a>(&self, options: &'a [OptionDescriptor]) -> Option<&'a OptionDescriptor> {
        self.as_str().find_in(options)
    }

    fn label(&self) -> String {
        self.clone()
    }
}

impl OptionId for OptionDescriptor {
    fn find_in<'a>(&self, options: &'a [OptionDescriptor]) -> Option<&'a OptionDescriptor> {
        self.index.find_in(options)
    }

    fn label(&self) -> String {
        OptionDescriptor::label(self)
    }
}

impl<T: OptionId + ?Sized> OptionId for &T {
    fn find_in<'a>(&self, options: &'a [OptionDescriptor]) -> Option<&'a OptionDescriptor> {
        (**self).find_in(options)
    }

    fn label(&self) -> String {
        (**self).label()
    }
}

/// The names of the well-known options and values of the SANE standard.
///
/// Backends are not required to provide these options, but most do, with
/// these names.
pub mod well_known {
    /// Int or fixed: the resolution of the scan, in dpi.
    pub const RESOLUTION: &str = "resolution";
    /// String: the scan mode, usually one of [`MODE_LINEART`],
    /// [`MODE_GRAY`] or [`MODE_COLOR`].
    pub const MODE: &str = "mode";
    /// String: the source of the documents (flatbed, feeder...).
    pub const SOURCE: &str = "source";
    /// Int: the number of bits per sample.
    pub const DEPTH: &str = "depth";
    /// Bool: request a fast, low resolution preview scan.
    pub const PREVIEW: &str = "preview";
    /// Int or fixed: the left edge of the scan area.
    pub const TL_X: &str = "tl-x";
    /// Int or fixed: the top edge of the scan area.
    pub const TL_Y: &str = "tl-y";
    /// Int or fixed: the right edge of the scan area.
    pub const BR_X: &str = "br-x";
    /// Int or fixed: the bottom edge of the scan area.
    pub const BR_Y: &str = "br-y";
    /// The line-art (1-bit black and white) scan mode.
    pub const MODE_LINEART: &str = "Lineart";
    /// The grayscale scan mode.
    pub const MODE_GRAY: &str = "Gray";
    /// The color scan mode.
    pub const MODE_COLOR: &str = "Color";
}

const WORD_SIZE: usize = size_of::<sys::SANE_Word>();

/// The memory a value is exchanged through with `sane_control_option`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Buffer {
    /// Bool, int and fixed values: an array of words.
    Words(Vec<sys::SANE_Word>),
    /// String values: a NUL-terminated string in a buffer of `size` bytes.
    Bytes(Vec<u8>),
}

impl Buffer {
    /// A zeroed buffer large enough for the value of `option`, or `None` for
    /// options without a value (buttons, groups).
    pub(crate) fn for_option(value_type: ValueType, size: usize) -> Option<Self> {
        match value_type {
            ValueType::Bool | ValueType::Int | ValueType::Fixed => {
                Some(Self::Words(vec![0; size.div_ceil(WORD_SIZE).max(1)]))
            }
            ValueType::String => Some(Self::Bytes(vec![0; size.max(1)])),
            _ => None,
        }
    }

    /// The size of the buffer in bytes.
    pub(crate) fn byte_len(&self) -> usize {
        match self {
            Self::Words(words) => words.len() * WORD_SIZE,
            Self::Bytes(bytes) => bytes.len(),
        }
    }

    /// Decodes the value of `option` read into this buffer.
    pub(crate) fn decode(&self, option: &OptionDescriptor) -> Result<Value> {
        let mismatch = || Error::TypeMismatch {
            option: option.label(),
            expected: option.value_type,
        };
        match (option.value_type, self) {
            (ValueType::Bool, Self::Words(words)) => {
                Ok(Value::Bool(words.first().is_some_and(|&word| word != 0)))
            }
            (ValueType::Int, Self::Words(words)) => {
                Ok(match &words[..option.len().min(words.len())] {
                    [value] => Value::Int(*value),
                    values => Value::IntArray(values.to_vec()),
                })
            }
            (ValueType::Fixed, Self::Words(words)) => {
                Ok(match &words[..option.len().min(words.len())] {
                    [value] => Value::Fixed(Fixed::from_raw(*value)),
                    values => {
                        Value::FixedArray(values.iter().copied().map(Fixed::from_raw).collect())
                    }
                })
            }
            (ValueType::String, Self::Bytes(bytes)) => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                Ok(Value::String(
                    String::from_utf8_lossy(&bytes[..end]).into_owned(),
                ))
            }
            _ => Err(mismatch()),
        }
    }

    /// Encodes `value` for `option`, checking its type and size.
    pub(crate) fn encode(option: &OptionDescriptor, value: &Value) -> Result<Self> {
        let mismatch = || Error::TypeMismatch {
            option: option.label(),
            expected: option.value_type,
        };
        let invalid = |reason: String| Error::InvalidValue {
            option: option.label(),
            reason,
        };
        let words = |words: Vec<sys::SANE_Word>| {
            if words.len() == option.len() {
                Ok(Self::Words(words))
            } else {
                Err(invalid(format!(
                    "{} element(s) given, the option holds {}",
                    words.len(),
                    option.len()
                )))
            }
        };
        match (option.value_type, value) {
            (ValueType::Bool, Value::Bool(value)) => words(vec![if *value {
                sys::SANE_TRUE
            } else {
                sys::SANE_FALSE
            } as sys::SANE_Word]),
            (ValueType::Int, Value::Int(value)) => words(vec![*value]),
            (ValueType::Int, Value::IntArray(values)) => words(values.clone()),
            (ValueType::Fixed, Value::Fixed(value)) => words(vec![value.to_raw()]),
            (ValueType::Fixed, Value::Int(value)) => {
                words(vec![Fixed::from_f64(f64::from(*value)).to_raw()])
            }
            (ValueType::Fixed, Value::FixedArray(values)) => {
                words(values.iter().map(|value| value.to_raw()).collect())
            }
            (ValueType::Fixed, Value::IntArray(values)) => words(
                values
                    .iter()
                    .map(|&value| Fixed::from_f64(f64::from(value)).to_raw())
                    .collect(),
            ),
            (ValueType::String, Value::String(value)) => {
                if value.as_bytes().contains(&0) {
                    return Err(Error::NulByte);
                }
                if value.len() >= option.size {
                    return Err(invalid(format!(
                        "the string is {} bytes long, the option holds at most {}",
                        value.len(),
                        option.size.saturating_sub(1)
                    )));
                }
                // Some backends copy `size` bytes: the whole buffer is
                // initialized, NUL-padded.
                let mut bytes = vec![0; option.size];
                bytes[..value.len()].copy_from_slice(value.as_bytes());
                Ok(Self::Bytes(bytes))
            }
            _ => Err(mismatch()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(value_type: ValueType, size: usize) -> OptionDescriptor {
        OptionDescriptor {
            index: 1,
            name: "opt".into(),
            title: String::new(),
            description: String::new(),
            value_type,
            unit: Unit::None,
            size,
            capabilities: Capabilities::SOFT_SELECT | Capabilities::SOFT_DETECT,
            constraint: Constraint::None,
        }
    }

    fn round_trip(option: &OptionDescriptor, value: Value) -> Value {
        Buffer::encode(option, &value)
            .unwrap()
            .decode(option)
            .unwrap()
    }

    #[test]
    fn scalar_round_trips() {
        let bool_option = option(ValueType::Bool, 4);
        assert_eq!(round_trip(&bool_option, true.into()), Value::Bool(true));
        assert_eq!(round_trip(&bool_option, false.into()), Value::Bool(false));
        let int_option = option(ValueType::Int, 4);
        assert_eq!(round_trip(&int_option, (-42).into()), Value::Int(-42));
        let fixed_option = option(ValueType::Fixed, 4);
        assert_eq!(
            round_trip(&fixed_option, 1.5.into()),
            Value::Fixed(Fixed::from_f64(1.5))
        );
        // Integers are accepted for fixed options.
        assert_eq!(
            round_trip(&fixed_option, 300.into()),
            Value::Fixed(Fixed::from(300))
        );
    }

    #[test]
    fn array_round_trips() {
        let int_array = option(ValueType::Int, 12);
        assert_eq!(int_array.len(), 3);
        assert_eq!(
            round_trip(&int_array, vec![1, 2, 3].into()),
            Value::IntArray(vec![1, 2, 3])
        );
        let fixed_array = option(ValueType::Fixed, 8);
        let values = vec![Fixed::from(1), Fixed::from_f64(-0.25)];
        assert_eq!(
            round_trip(&fixed_array, values.clone().into()),
            Value::FixedArray(values)
        );
        assert_eq!(
            round_trip(&fixed_array, vec![1, 2].into()),
            Value::FixedArray(vec![Fixed::from(1), Fixed::from(2)])
        );
    }

    #[test]
    fn string_round_trips_and_is_padded() {
        let string_option = option(ValueType::String, 8);
        let buffer = Buffer::encode(&string_option, &"Color".into()).unwrap();
        assert_eq!(buffer, Buffer::Bytes(b"Color\0\0\0".to_vec()));
        assert_eq!(
            buffer.decode(&string_option).unwrap(),
            Value::String("Color".into())
        );
        // A buffer without NUL is read whole.
        assert_eq!(
            Buffer::Bytes(b"abc".to_vec())
                .decode(&string_option)
                .unwrap(),
            Value::String("abc".into())
        );
    }

    #[test]
    fn encode_rejects_bad_values() {
        let string_option = option(ValueType::String, 4);
        assert!(matches!(
            Buffer::encode(&string_option, &"long".into()),
            Err(Error::InvalidValue { .. })
        ));
        assert_eq!(
            Buffer::encode(&string_option, &"a\0".into()),
            Err(Error::NulByte)
        );
        assert!(matches!(
            Buffer::encode(&string_option, &1.into()),
            Err(Error::TypeMismatch {
                expected: ValueType::String,
                ..
            })
        ));
        let int_array = option(ValueType::Int, 8);
        assert!(matches!(
            Buffer::encode(&int_array, &1.into()),
            Err(Error::InvalidValue { .. })
        ));
        assert!(matches!(
            Buffer::encode(&option(ValueType::Int, 4), &1.5.into()),
            Err(Error::TypeMismatch { .. })
        ));
        assert!(matches!(
            Buffer::encode(&option(ValueType::Button, 0), &true.into()),
            Err(Error::TypeMismatch { .. })
        ));
    }

    #[test]
    fn buffers_cover_the_option_size() {
        assert_eq!(
            Buffer::for_option(ValueType::Int, 12).unwrap().byte_len(),
            12
        );
        assert_eq!(Buffer::for_option(ValueType::Int, 0).unwrap().byte_len(), 4);
        assert_eq!(
            Buffer::for_option(ValueType::Fixed, 5).unwrap().byte_len(),
            8
        );
        assert_eq!(
            Buffer::for_option(ValueType::String, 32)
                .unwrap()
                .byte_len(),
            32
        );
        assert_eq!(Buffer::for_option(ValueType::Button, 0), None);
        assert_eq!(Buffer::for_option(ValueType::Group, 0), None);
    }

    #[test]
    fn option_ids() {
        let mut first = option(ValueType::Int, 4);
        first.name = "resolution".into();
        let mut group = option(ValueType::Group, 0);
        group.index = 2;
        group.name = String::new();
        let options = [first, group];
        assert_eq!(1.find_in(&options).unwrap().name, "resolution");
        assert_eq!("resolution".find_in(&options).unwrap().index, 1);
        assert_eq!(
            String::from("resolution").find_in(&options).unwrap().index,
            1
        );
        assert_eq!(options[1].find_in(&options).unwrap().index, 2);
        assert!("".find_in(&options).is_none());
        assert!(3.find_in(&options).is_none());
    }

    #[test]
    fn capabilities() {
        let caps = Capabilities::SOFT_SELECT | Capabilities::SOFT_DETECT;
        assert!(caps.is_active() && caps.is_settable() && caps.is_readable());
        assert!(!(caps | Capabilities::INACTIVE).is_active());
        assert_eq!(
            format!("{:?}", Capabilities::from_bits(0x101)),
            "{SOFT_SELECT, 0x100}"
        );
        let info = SetInfo::from_raw(sys::SANE_INFO_RELOAD_OPTIONS as i32);
        assert!(info.reload_options() && !info.inexact() && !info.reload_params());
    }
}

//! A count of cards that is always at least one.

use std::fmt;
use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

/// A count of cards that is always at least one.
///
/// Decoding a zero or negative quantity is an error, and arithmetic that
/// would reach zero returns `None` instead of producing an empty value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Quantity(NonZeroU32);

impl Quantity {
    /// One card.
    pub const ONE: Self = Self(NonZeroU32::MIN);

    /// Wraps a positive count.
    #[must_use]
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }

    /// A positive count, or one when the source value is missing, zero, or
    /// negative. This is how both apps treat quantities from remote deck
    /// sites (`Util.positive_quantity/1`, `Decklist.card/4`).
    #[must_use]
    pub fn or_one(value: Option<i64>) -> Self {
        value
            .and_then(|value| u32::try_from(value).ok())
            .and_then(Self::new)
            .unwrap_or(Self::ONE)
    }

    /// The count.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// The count as a SQLite integer.
    #[must_use]
    pub fn as_i64(self) -> i64 {
        i64::from(self.get())
    }

    /// `self - other`, or `None` when nothing would remain.
    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.get().checked_sub(other.get()).and_then(Self::new)
    }

    /// `self + other`, saturating at `u32::MAX`.
    #[must_use]
    pub fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.get()))
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<Quantity> for u32 {
    fn from(quantity: Quantity) -> Self {
        quantity.get()
    }
}

impl From<Quantity> for i64 {
    fn from(quantity: Quantity) -> Self {
        quantity.as_i64()
    }
}

/// Raised when a stored or requested quantity is not a positive count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("quantity must be a positive count, got {0}")]
pub struct InvalidQuantity(pub i64);

impl TryFrom<u32> for Quantity {
    type Error = InvalidQuantity;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidQuantity(i64::from(value)))
    }
}

impl TryFrom<i64> for Quantity {
    type Error = InvalidQuantity;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .ok()
            .and_then(Self::new)
            .ok_or(InvalidQuantity(value))
    }
}

#[cfg(feature = "sqlx")]
mod sqlx_impl {
    use sqlx::error::BoxDynError;
    use sqlx::sqlite::{SqliteArgumentsBuffer, SqliteTypeInfo, SqliteValueRef};
    use sqlx::{Decode, Encode, Sqlite, Type};

    use super::Quantity;

    impl Type<Sqlite> for Quantity {
        fn type_info() -> SqliteTypeInfo {
            <i64 as Type<Sqlite>>::type_info()
        }

        fn compatible(ty: &SqliteTypeInfo) -> bool {
            <i64 as Type<Sqlite>>::compatible(ty)
        }
    }

    impl<'r> Decode<'r, Sqlite> for Quantity {
        fn decode(value: SqliteValueRef<'r>) -> Result<Self, BoxDynError> {
            let raw = <i64 as Decode<Sqlite>>::decode(value)?;
            Ok(Quantity::try_from(raw)?)
        }
    }

    impl Encode<'_, Sqlite> for Quantity {
        fn encode_by_ref(
            &self,
            buf: &mut SqliteArgumentsBuffer,
        ) -> Result<sqlx::encode::IsNull, BoxDynError> {
            <i64 as Encode<Sqlite>>::encode_by_ref(&self.as_i64(), buf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_and_negative() {
        assert_eq!(Quantity::new(0), None);
        assert_eq!(Quantity::try_from(-3_i64), Err(InvalidQuantity(-3)));
        assert_eq!(Quantity::try_from(2_i64).unwrap().get(), 2);
    }

    #[test]
    fn or_one_defaults_bad_values() {
        assert_eq!(Quantity::or_one(None), Quantity::ONE);
        assert_eq!(Quantity::or_one(Some(0)), Quantity::ONE);
        assert_eq!(Quantity::or_one(Some(-1)), Quantity::ONE);
        assert_eq!(Quantity::or_one(Some(4)).get(), 4);
    }

    #[test]
    fn arithmetic_never_reaches_zero() {
        let three = Quantity::new(3).unwrap();
        assert_eq!(three.checked_sub(Quantity::ONE).unwrap().get(), 2);
        assert_eq!(three.checked_sub(three), None);
        assert_eq!(three.saturating_add(Quantity::ONE).get(), 4);
    }

    #[test]
    fn serde_rejects_zero() {
        assert!(serde_json::from_str::<Quantity>("0").is_err());
        assert_eq!(serde_json::from_str::<Quantity>("5").unwrap().get(), 5);
        assert_eq!(serde_json::to_string(&Quantity::ONE).unwrap(), "1");
    }
}

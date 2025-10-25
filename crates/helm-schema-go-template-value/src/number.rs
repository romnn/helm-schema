use std::cmp::{Ordering, PartialOrd};
use std::fmt;
use std::{f32, f64, i64, u64};

/// Internal number format for `gtmpl_value`.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Number {
    n: Num,
}

#[cfg(feature = "serde")]
impl serde::Serialize for Number {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.n.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Number {
    fn deserialize<D>(deserializer: D) -> Result<Number, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Number {
            n: Num::deserialize(deserializer)?,
        })
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
#[derive(Copy, Clone, Debug)]
enum Num {
    U(u64),
    I(i64),
    F(f64),
}

/// `PartialOrd` for `Number`.
///
/// # Examples
///
/// ```rust
/// use gtmpl_value::Number;
///
/// let i: Number = 23.into();
/// let f: Number = 23.42.into();
///
/// assert!(i < f);
/// ```
impl PartialOrd for Num {
    fn partial_cmp(&self, other: &Num) -> Option<Ordering> {
        match (*self, *other) {
            (Num::U(s), Num::U(o)) => s.partial_cmp(&o),
            (Num::I(s), Num::I(o)) => s.partial_cmp(&o),
            (Num::F(s), Num::F(o)) => s.partial_cmp(&o),
            (Num::I(_), Num::U(_)) => Some(Ordering::Less),
            (Num::U(_), Num::I(_)) => Some(Ordering::Greater),
            (Num::F(s), Num::I(o)) => s.partial_cmp(&(o as f64)),
            (Num::I(s), Num::F(o)) => (s as f64).partial_cmp(&o),
            (Num::F(s), Num::U(o)) => s.partial_cmp(&(o as f64)),
            (Num::U(s), Num::F(o)) => (s as f64).partial_cmp(&o),
        }
    }
}

/// `PartialEq` for `Number`.
///
/// # Examples
///
/// ```rust
/// use gtmpl_value::Number;
///
/// let i: Number = 23.into();
/// let f: Number = 23.0.into();
///
/// assert!(i == f);
/// ```
impl PartialEq for Num {
    fn eq(&self, other: &Num) -> bool {
        match (*self, *other) {
            (Num::U(s), Num::U(o)) => s.eq(&o),
            (Num::I(s), Num::I(o)) => s.eq(&o),
            (Num::F(s), Num::F(o)) => s.eq(&o),
            _ => false,
        }
    }
}

impl Number {
    /// ```rust
    /// use std::i64;
    /// use gtmpl_value::Number;
    ///
    /// let big: Number = (i64::MAX as u64 + 10).into();
    ///
    /// assert!(big.as_u64().is_some());
    /// assert!(big.as_i64().is_none());
    /// assert!(big.as_f64().is_none());
    /// ```
    pub fn as_i64(&self) -> Option<i64> {
        match self.n {
            Num::U(n) => {
                if n <= (i64::MAX as u64) {
                    Some(n as i64)
                } else {
                    None
                }
            }
            Num::I(n) => Some(n),
            _ => None,
        }
    }
    /// ```rust
    /// use std::i64;
    /// use gtmpl_value::Number;
    ///
    /// let neg: Number = (-10).into();
    ///
    /// assert!(neg.as_u64().is_none());
    /// assert!(neg.as_i64().is_some());
    /// assert!(neg.as_f64().is_none());
    /// ```
    pub fn as_u64(&self) -> Option<u64> {
        match self.n {
            Num::U(n) => Some(n),
            Num::I(n) => {
                if n >= 0 {
                    Some(n as u64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// ```rust
    /// use std::i64;
    /// use gtmpl_value::Number;
    ///
    /// let frac: Number = (10.1).into();
    ///
    /// assert!(frac.as_u64().is_none());
    /// assert!(frac.as_i64().is_none());
    /// assert!(frac.as_f64().is_some());
    /// ```
    pub fn as_f64(&self) -> Option<f64> {
        match self.n {
            Num::F(n) => Some(n),
            _ => None,
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.n {
            Num::U(n) => write!(f, "{}", n),
            Num::I(n) => write!(f, "{}", n),
            Num::F(n) => write!(f, "{}", n),
        }
    }
}

macro_rules! from_i {
    ($($ty:ident)*) => {
        $(
            impl From<$ty> for Number {
                fn from(n: $ty) -> Self {
                    Number {
                        n: if n < 0 { Num::I(i64::from(n)) } else { Num::U(n as u64) }
                    }
                }
            }
        )*
    };
}

from_i!(
    i64 i32 i16 i8
);

impl From<isize> for Number {
    fn from(n: isize) -> Self {
        Number {
            n: if n < 0 {
                Num::I(n as i64)
            } else {
                Num::U(n as u64)
            },
        }
    }
}

macro_rules! from_u {
    ($($ty:ident)*) => {
        $(
            impl From<$ty> for Number {
                fn from(n: $ty) -> Self {
                    Number {
                        n: Num::U(u64::from(n)),
                    }
                }
            }
        )*
    };
}

from_u!(
    u64 u32 u16 u8
);

impl From<usize> for Number {
    fn from(n: usize) -> Self {
        Number {
            n: Num::U(n as u64),
        }
    }
}

macro_rules! from_f {
    ($($ty:ident)*) => {
        $(
            impl From<$ty> for Number {
                fn from(n: $ty) -> Self {
                    let num = match n {
                        n if n.fract().abs() < $ty::EPSILON => {
                            if n.is_sign_negative() { Num::I(n as i64) } else { Num::U(n as u64) }
                        },
                        n => Num::F(f64::from(n)),
                    };
                    Number {
                        n: num,
                    }
                }
            }
        )*
    };
}

from_f!(
    f64 f32
);

#[cfg(test)]
mod test {
    use super::*;
    use std::u64;

    #[test]
    fn test_i() {
        let num: Number = (-23i8).into();
        assert_eq!(num.as_i64(), Some(-23i64));
        assert_eq!(num.as_u64(), None);
        assert_eq!(num.as_f64(), None);
    }

    #[test]
    fn test_u() {
        let num: Number = 23u8.into();
        assert_eq!(num.as_i64(), Some(23i64));
        assert_eq!(num.as_u64(), Some(23u64));
        assert_eq!(num.as_f64(), None);
    }

    #[test]
    fn test_u_max() {
        let num: Number = u64::MAX.into();
        assert_eq!(num.as_i64(), None);
        assert_eq!(num.as_u64(), Some(u64::MAX));
        assert_eq!(num.as_f64(), None);
    }

    #[test]
    fn test_f() {
        let num: Number = 23.42f64.into();
        assert_eq!(num.as_i64(), None);
        assert_eq!(num.as_u64(), None);
        assert_eq!(num.as_f64(), Some(23.42f64));
        let num: Number = (-23.42f64).into();
        assert_eq!(num.as_i64(), None);
        assert_eq!(num.as_u64(), None);
        assert_eq!(num.as_f64(), Some(-23.42f64));
    }

    #[test]
    fn test_le() {
        let a: Number = 23.0f64.into();
        let b: Number = 24u64.into();
        assert!(a <= b);
    }

    #[test]
    fn test_ge() {
        let a: Number = 1u64.into();
        let b: Number = (-1i64).into();
        assert!(a > b);
    }
}

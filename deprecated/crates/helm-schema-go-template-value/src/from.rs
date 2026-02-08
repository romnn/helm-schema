use std::borrow::Cow;
use std::collections::HashMap;

use crate::value::{Func, Function, Value};

macro_rules! from_num {
    ($($ty:ident)*) => {
        $(
            impl From<$ty> for Value {
                fn from(n: $ty) -> Self {
                    Value::Number(n.into())
                }
            }
        )*
    };
}

from_num! {
    i8 i16 i32 i64 isize
    u8 u16 u32 u64 usize
    f32 f64
}

impl From<bool> for Value {
    /// Convert boolean to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    ///
    /// let b = false;
    /// let x: Value = b.into();
    /// ```
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl<'a> From<&'a String> for Value {
    /// Convert &String to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    ///
    /// let s: &String = &"foobar".to_owned();
    /// let x: Value = s.into();
    /// ```
    fn from(s: &'a String) -> Self {
        Value::String(s.clone())
    }
}

impl From<String> for Value {
    /// Convert String to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    ///
    /// let s: String = "foobar".to_owned();
    /// let x: Value = s.into();
    /// ```
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl<'a> From<&'a str> for Value {
    /// Convert &str to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    ///
    /// let s = "foobar";
    /// let x: Value = s.into();
    /// ```
    fn from(f: &str) -> Self {
        Value::String(f.to_string())
    }
}

impl<'a> From<Cow<'a, str>> for Value {
    /// Convert Cow<str> to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    /// use std::borrow::Cow;
    ///
    /// let s: Cow<str> = Cow::Borrowed("foobar");
    /// let x: Value = s.into();
    /// ```
    fn from(f: Cow<'a, str>) -> Self {
        Value::String(f.to_string())
    }
}

impl From<Func> for Value {
    /// Convert Func to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::{Func, FuncError, Value};
    ///
    /// fn f(a: &[Value]) -> Result<Value, FuncError> {
    ///     Ok(a[0].clone())
    /// };
    /// let x: Value = (f as Func).into();
    /// ```
    fn from(f: Func) -> Self {
        Value::Function(Function { f })
    }
}

impl<T> From<Vec<T>> for Value
where
    T: Into<Value> + Clone,
{
    /// Convert Vec to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    ///
    /// let v = vec!(1, 2, 3);
    /// let x: Value = v.into();
    /// ```
    fn from(f: Vec<T>) -> Self {
        Value::Array(f.iter().cloned().map(|x| x.into()).collect())
    }
}

impl<'a, T> From<&'a [T]> for Value
where
    T: Into<Value> + Clone,
{
    /// Convert Slice to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    ///
    /// let v: &[i32] = &[1, 2, 3];
    /// let x: Value = v.into();
    /// ```
    fn from(f: &'a [T]) -> Self {
        Value::Array(f.iter().cloned().map(|x| x.into()).collect())
    }
}

impl<T> From<HashMap<String, T>> for Value
where
    T: Into<Value> + Clone,
{
    /// Convert HashMap<String, T> to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    /// use std::collections::HashMap;
    ///
    /// let mut m = HashMap::new();
    /// m.insert("hello".to_owned(), 123);
    /// let x: Value = m.into();
    /// ```
    fn from(f: HashMap<String, T>) -> Self {
        Value::Map(
            f.iter()
                .map(|(s, x)| (s.clone(), x.clone().into()))
                .collect(),
        )
    }
}

impl<T> From<Option<T>> for Value
where
    T: Into<Value> + Clone,
{
    /// Convert Option<T> to `Value`
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gtmpl_value::Value;
    ///
    /// let i = Some(1);
    /// let x: Value = i.into();
    /// ```
    fn from(f: Option<T>) -> Self {
        match f {
            Some(x) => x.into(),
            _ => Value::NoValue,
        }
    }
}

/// Convert Value into something.
pub trait FromValue<T> {
    /// Tries to retrieve `T` from `Value.`
    fn from_value(val: &Value) -> Option<T>;
}

impl FromValue<i64> for i64 {
    /// Tries to retrieve `i64` from `Value.`
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use gtmpl_value::{FromValue, Value};
    ///
    /// let v: Value = 23i64.into();
    /// let i = i64::from_value(&v);
    /// assert_eq!(i, Some(23i64));
    /// ```
    fn from_value(val: &Value) -> Option<i64> {
        if let Value::Number(ref n) = *val {
            n.as_i64()
        } else {
            None
        }
    }
}

impl FromValue<u64> for u64 {
    /// Tries to retrieve `u64` from `Value.`
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use gtmpl_value::{FromValue, Value};
    ///
    /// let v: Value = 23u64.into();
    /// let i = u64::from_value(&v);
    /// assert_eq!(i, Some(23u64));
    /// ```
    fn from_value(val: &Value) -> Option<u64> {
        if let Value::Number(ref n) = *val {
            n.as_u64()
        } else {
            None
        }
    }
}

impl FromValue<f64> for f64 {
    /// Tries to retrieve `f64` from `Value.`
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use gtmpl_value::{FromValue, Value};
    ///
    /// let v: Value = 23.1f64.into();
    /// let i = f64::from_value(&v);
    /// assert_eq!(i, Some(23.1f64));
    /// ```
    fn from_value(val: &Value) -> Option<f64> {
        if let Value::Number(ref n) = *val {
            n.as_f64()
        } else {
            None
        }
    }
}

impl FromValue<String> for String {
    /// Tries to retrieve `String` from `Value.`
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use gtmpl_value::{FromValue, Value};
    ///
    /// let v: Value = "foobar".into();
    /// let s = String::from_value(&v);
    /// assert_eq!(s, Some("foobar".to_owned()));
    /// ```
    fn from_value(val: &Value) -> Option<String> {
        if let Value::String(ref s) = *val {
            Some(s.clone())
        } else {
            None
        }
    }
}

impl<T> FromValue<Vec<T>> for Vec<T>
where
    T: FromValue<T>,
{
    /// Tries to retrieve `Vec<T>` from `Value.`
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use gtmpl_value::{FromValue, Value};
    ///
    /// let v: Value = vec!(1, 2, 3).into();
    /// let v: Option<Vec<i64>> = Vec::from_value(&v);
    /// assert_eq!(v, Some(vec!(1, 2, 3)));
    /// ```
    fn from_value(val: &Value) -> Option<Vec<T>> {
        if let Value::Array(ref a) = *val {
            let v: Vec<T> = a.iter().flat_map(|v| T::from_value(v)).collect();
            if v.len() == a.len() {
                return Some(v);
            }
        }
        None
    }
}

#[allow(clippy::implicit_hasher)]
impl<T> FromValue<HashMap<String, T>> for HashMap<String, T>
where
    T: FromValue<T>,
{
    /// Tries to retrieve `HashMap<String, T>` from `Value.`
    ///
    /// # Examples:
    ///
    /// ```rust
    /// use gtmpl_value::{FromValue, Value};
    /// use std::collections::HashMap;
    ///
    /// let mut m = HashMap::new();
    /// m.insert("a".to_owned(), 1);
    /// let v: Value = m.into();
    /// let m: Option<HashMap<String, i64>> = HashMap::from_value(&v);
    /// assert!(m.is_some());
    /// if let Some(m) = m {
    ///   assert_eq!(m.get("a"), Some(&1));
    /// }
    /// ```
    fn from_value(val: &Value) -> Option<HashMap<String, T>> {
        match *val {
            Value::Object(ref o) | Value::Map(ref o) => {
                let m: HashMap<String, T> = o
                    .iter()
                    .map(|(s, v)| (s.clone(), T::from_value(v)))
                    .flat_map(|(s, t)| t.map(|t| (s, t)))
                    .collect();
                if m.len() == o.len() {
                    Some(m)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// `FromValue` wrapped in a macro (required for `gtmpl_fn!` macro).
///
/// # Examples:
///
/// ```rust
/// use gtmpl_value::{from_value, Value};
///
/// let v: Value = 1.into();
/// let s: Option<i64> = from_value(&v);
/// assert_eq!(s, Some(1));
/// ```
pub fn from_value<T>(val: &Value) -> Option<T>
where
    T: FromValue<T>,
{
    T::from_value(val)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_vec() {
        let val: Value = vec![1, 2, 3].into();
        if let Value::Array(array) = val {
            assert_eq!(array[0], 1.into());
            assert_eq!(array[1], 2.into());
            assert_eq!(array[2], 3.into());
        } else {
            panic!();
        }

        let val: Value = vec!["foo", "bar"].into();
        if let Value::Array(array) = val {
            assert_eq!(array[0], "foo".into());
            assert_eq!(array[1], "bar".into());
        } else {
            panic!();
        }
    }

    #[test]
    fn test_slice() {
        let slice: &[u8] = &[1, 2, 3];
        let val: Value = slice.into();
        if let Value::Array(array) = val {
            assert_eq!(array[0], 1.into());
            assert_eq!(array[1], 2.into());
            assert_eq!(array[2], 3.into());
        } else {
            panic!();
        }
    }

    #[test]
    fn test_map() {
        let mut m = HashMap::new();
        m.insert("a".to_owned(), 1);
        m.insert("b".to_owned(), 2);
        let val: Value = m.into();
        if let Value::Map(obj) = val {
            assert_eq!(obj.get("a"), Some(&(1.into())));
            assert_eq!(obj.get("b"), Some(&(2.into())));
        } else {
            panic!();
        }
    }
}

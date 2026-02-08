//! `gtmpl_value` is a basic implementation for internal values within
//! [`gtmpl-rust`][gtmpl_value-github]. It is used as to represent values parsed from
//! the template and from the context.
//!
//! [gtmpl_value-github]: https://github.com/fiji-flo/gtmpl-rust
//!
//! The [`From`](https://doc.rust-lang.org/std/convert/trait.From.html) trait is
//! implemented for:
//!
//! * `String, &str`
//! * most numeric types `u64, u32, …, i64, i32, …, f64, f32`
//! * `bool`
//! * `Vec<Value>, &[Value]`
//! * `HashMap<String, Value>`
//!
//! [`gtmpl_derive`](https://github.com/fiji-flo/gtmpl_derive) provides a custom
//! `derive` for structs.
//!
//! # Examples
//!
//! ```rust
//! extern crate gtmpl_value;
//! use gtmpl_value::Value;
//!
//! fn main() {
//!     let v: Value = "something".into();
//!     println!("{}", v);
//! }
//! ```
#![allow(warnings)]

mod from;
mod number;
mod value;

pub use crate::from::*;
pub use crate::value::*;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_function_cmp() {
        fn f(a: &[Value]) -> Result<Value, FuncError> {
            Ok(a[0].clone())
        }
        let f1 = Function { f };
        let f2 = Function { f };
        assert_eq!(f1, f2);
    }
}

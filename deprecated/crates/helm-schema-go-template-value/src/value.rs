use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

#[doc(inline)]
pub use crate::number::Number;

#[derive(Debug, Error)]
pub enum FuncError {
    #[error("unable to convert argument from value")]
    UnableToConvertFromValue,
    #[error("{0} requires at least {1} argument(s)")]
    AtLeastXArgs(String, usize),
    #[error("{0} requires exactly {1} argument(s)")]
    ExactlyXArgs(String, usize),
    #[error("{0}")]
    Generic(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Function type supported by `gtmpl_value`.
pub type Func = fn(&[Value]) -> Result<Value, FuncError>;

/// Wrapper struct for `Func`.
#[derive(Clone)]
pub struct Function {
    pub f: Func,
}

impl PartialEq for Function {
    fn eq(&self, other: &Function) -> bool {
        self.f as fn(_) -> _ == other.f as fn(_) -> _
    }
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Function")
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Function")
    }
}

/// Represents a gtmpl value.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum Value {
    NoValue,
    Nil,
    Bool(bool),
    String(String),
    #[cfg_attr(feature = "serde", serde(skip))]
    Object(HashMap<String, Value>),
    Map(HashMap<String, Value>),
    Array(Vec<Value>),
    #[cfg_attr(feature = "serde", serde(skip))]
    Function(Function),
    Number(Number),
}

impl Value {
    pub fn from<T>(t: T) -> Self
    where
        T: Into<Value>,
    {
        t.into()
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Value::NoValue => write!(f, "<no value>"),
            Value::Nil => write!(f, "nil"),
            Value::Bool(ref b) => write!(f, "{}", b),
            Value::String(ref s) => write!(f, "{}", s),
            Value::Function(ref func) => write!(f, "{}", func),
            Value::Number(ref n) => write!(f, "{}", n),
            Value::Array(ref a) => write!(f, "{:?}", a),
            Value::Object(ref o) => write!(f, "{:?}", o),
            Value::Map(ref m) => write!(f, "{:?}", m),
        }
    }
}

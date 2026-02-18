//! The Golang Templating Language for Rust.
//!
//! ## Example
//! ```rust
//! use gtmpl;
//!
//! let output = gtmpl::template("Finally! Some {{ . }} for Rust", "gtmpl");
//! assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
//! ```
#![allow(warnings)]

pub mod error;
pub mod exec;
pub mod funcs;
pub mod lexer;
pub mod node;
pub mod parse;
pub mod print_verb;
pub mod printf;
pub mod template;
pub mod utils;

#[doc(inline)]
pub use crate::template::Template;

#[doc(inline)]
pub use crate::exec::Context;

#[doc(inline)]
pub use helm_schema_go_template_value::Func;

pub use helm_schema_go_template_value::FuncError;

#[doc(inline)]
pub use helm_schema_go_template_value::from_value;

pub use error::TemplateError;
pub use helm_schema_go_template_value::Value;

/// Provides simple basic templating given just a template sting and context.
///
/// ## Example
/// ```rust
/// let output = gtmpl::template("Finally! Some {{ . }} for Rust", "gtmpl");
/// assert_eq!(&output.unwrap(), "Finally! Some gtmpl for Rust");
/// ```
pub fn template<T: Into<Value>>(template_str: &str, context: T) -> Result<String, TemplateError> {
    let mut tmpl = Template::default();
    tmpl.parse(template_str)?;
    tmpl.render(&Context::from(context)).map_err(Into::into)
}

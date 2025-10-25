use gtmpl_derive::Gtmpl;
use gtmpl_value::{Func, FuncError, Value};
use std::collections::HashMap;

#[test]
fn test1() {
    #[derive(Gtmpl)]
    struct Foo {
        bar: String,
    }

    let val = Value::from(Foo {
        bar: "2000".to_owned(),
    });
    if let Value::Object(ref m) = val {
        assert_eq!(m.get("bar"), Some(&Value::String("2000".to_owned())));
    } else {
        assert!(false);
    }
}

#[test]
fn test2() {
    #[derive(Gtmpl)]
    struct Bar {
        i: i64,
    }
    #[derive(Gtmpl)]
    struct Foo {
        bar: Bar,
    }

    let val = Value::from(Foo { bar: Bar { i: 23 } });
    if let Value::Object(ref m) = val {
        if let Some(&Value::Object(ref o)) = m.get("bar") {
            if let Some(&Value::Number(ref n)) = o.get("i") {
                return assert_eq!(n.as_i64(), Some(23));
            }
        }
    }
    assert!(false);
}

#[test]
fn test3() {
    fn bar(a: &[Value]) -> Result<Value, FuncError> {
        Ok(a[0].clone())
    }

    #[derive(Gtmpl)]
    struct Foo {
        bar: Func,
    }

    let val = Value::from(Foo { bar });
    let param: &[Value] = &[Value::from(23i64)];
    if let Value::Object(ref m) = val {
        if let Some(&Value::Function(ref f)) = m.get("bar") {
            let res = (f.f)(param).unwrap();
            if let Value::Number(ref i) = res {
                return assert_eq!(i.as_i64(), Some(23));
            }
        }
    }
    assert!(false);
}

#[test]
fn test4() {
    #[derive(Gtmpl)]
    struct Foo {
        field: HashMap<String, u64>,
    }
    let field = [("2000".to_owned(), 23u64)].iter().cloned().collect();
    let val = Value::from(Foo { field });
    if let Value::Object(ref m) = val {
        let map: HashMap<String, u64> = [("2000".to_owned(), 23u64)].iter().cloned().collect();
        let map: Value = map.into();
        if let Value::Map(_) = map {
            assert_eq!(m.get("field"), Some(&map));
        } else {
            assert!(false);
        }
    } else {
        assert!(false);
    }
}

#[test]
fn test5() {
    #[derive(Gtmpl, Clone)]
    struct Foo {
        name: String,
        value: i64,
    }
    #[derive(Gtmpl)]
    struct Bar {
        fields: Vec<Foo>,
    }
    let val = Value::from(Bar {
        fields: vec![Foo {
            name: "something".to_owned(),
            value: 23i64,
        }],
    });
    if let Value::Object(ref m) = val {
        assert!(m.get("fields").is_some());
    } else {
        assert!(false);
    }
}

#[test]
fn test6() {
    #[derive(Gtmpl)]
    struct Foo {
        field: Option<String>,
    }
    let val = Value::from(Foo {
        field: Some("something".to_owned()),
    });
    if let Value::Object(ref m) = val {
        assert!(m.get("field").is_some());
    } else {
        assert!(false);
    }
}

#[test]
fn test7() {
    #[derive(Gtmpl)]
    struct Foo {
        field: Option<String>,
    }
    let val = Value::from(Foo { field: None });
    if let Value::Object(ref m) = val {
        assert_eq!(m.get("field"), Some(&Value::NoValue));
    } else {
        assert!(false);
    }
}

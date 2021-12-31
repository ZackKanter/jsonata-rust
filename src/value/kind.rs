use std::collections::HashMap;

use crate::json::Number;

#[derive(Default, Debug)]
pub struct ArrayProps {
    is_sequence: bool,
    keep_singleton: bool,
    cons: bool,
}

#[derive(Debug)]
pub enum ValueKind {
    Undefined,
    Null,
    Number(Number),
    Bool(bool),
    String(String),
    Array(Vec<usize>, ArrayProps),
    Object(HashMap<String, usize>),
}

impl PartialEq<ValueKind> for ValueKind {
    fn eq(&self, other: &ValueKind) -> bool {
        match (self, other) {
            (Self::Number(l0), Self::Number(r0)) => l0 == r0,
            (Self::Bool(l0), Self::Bool(r0)) => l0 == r0,
            (Self::String(l0), Self::String(r0)) => l0 == r0,
            (Self::Array(l0, ..), Self::Array(r0, ..)) => l0 == r0,
            (Self::Object(l0), Self::Object(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl PartialEq<i32> for ValueKind {
    fn eq(&self, other: &i32) -> bool {
        match *self {
            ValueKind::Number(ref n) => *n == *other,
            _ => false,
        }
    }
}

impl PartialEq<i64> for ValueKind {
    fn eq(&self, other: &i64) -> bool {
        match *self {
            ValueKind::Number(ref n) => *n == *other,
            _ => false,
        }
    }
}

impl PartialEq<f32> for ValueKind {
    fn eq(&self, other: &f32) -> bool {
        match *self {
            ValueKind::Number(ref n) => *n == *other,
            _ => false,
        }
    }
}

impl PartialEq<f64> for ValueKind {
    fn eq(&self, other: &f64) -> bool {
        match *self {
            ValueKind::Number(ref n) => *n == *other,
            _ => false,
        }
    }
}

impl PartialEq<bool> for ValueKind {
    fn eq(&self, other: &bool) -> bool {
        match *self {
            ValueKind::Bool(ref b) => *b == *other,
            _ => false,
        }
    }
}

impl PartialEq<&str> for ValueKind {
    fn eq(&self, other: &&str) -> bool {
        match *self {
            ValueKind::String(ref s) => s == *other,
            _ => false,
        }
    }
}

impl PartialEq<String> for ValueKind {
    fn eq(&self, other: &String) -> bool {
        match *self {
            ValueKind::String(ref s) => *s == *other,
            _ => false,
        }
    }
}

impl From<i32> for ValueKind {
    fn from(v: i32) -> Self {
        ValueKind::Number(v.into())
    }
}

impl From<i64> for ValueKind {
    fn from(v: i64) -> Self {
        ValueKind::Number(v.into())
    }
}

impl From<f32> for ValueKind {
    fn from(v: f32) -> Self {
        ValueKind::Number(v.into())
    }
}

impl From<f64> for ValueKind {
    fn from(v: f64) -> Self {
        ValueKind::Number(v.into())
    }
}

impl From<bool> for ValueKind {
    fn from(v: bool) -> Self {
        ValueKind::Bool(v)
    }
}

impl From<&str> for ValueKind {
    fn from(v: &str) -> Self {
        ValueKind::String(v.into())
    }
}

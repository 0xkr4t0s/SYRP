//! The serde mapping shared by B6 (JSON) and B7 (the public `serde` impls).
//!
//! Both directions track nesting depth, so neither recursion can overflow the stack: reading
//! stops at [`MAX_DEPTH`] (before `serde_json`'s own limit of 128, which stays enabled), and
//! writing stops there too, which also catches objects that contain themselves.

use std::cell::Cell;
use std::fmt;

use serde_core::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_core::ser::{self, SerializeMap, SerializeSeq};
use serde_core::{Deserializer, Serialize, Serializer};

use crate::json::MAX_DEPTH;
use crate::{DynamicObject, Identifier, Var, VarView};

/// A `Var` being written at a given nesting depth.
pub(crate) struct Depth<'a> {
    value: &'a Var,
    depth: usize,
}

impl<'a> Depth<'a> {
    /// Wraps a top-level value.
    pub(crate) fn root(value: &'a Var) -> Self {
        Self { value, depth: 0 }
    }

    /// Fails when this value, an array or object, would be the container past [`MAX_DEPTH`].
    /// Containers are counted exactly as [`Seed::nested`] counts them when reading, so
    /// everything that is written can be read back.
    fn container<E: ser::Error>(&self) -> Result<(), E> {
        if self.depth >= MAX_DEPTH {
            return Err(E::custom(format_args!(
                "value nests more than {MAX_DEPTH} levels deep or contains a cycle"
            )));
        }
        Ok(())
    }

    /// Wraps a value nested one level below this one.
    fn child(&self, value: &'a Var) -> Depth<'a> {
        Depth {
            value,
            depth: self.depth + 1,
        }
    }
}

impl Serialize for Depth<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.value.as_view() {
            VarView::Void | VarView::Undefined => s.serialize_unit(),
            VarView::Bool(b) => s.serialize_bool(b),
            VarView::Int(n) => s.serialize_i32(n),
            VarView::Int64(n) => s.serialize_i64(n),
            VarView::Double(x) => s.serialize_f64(x),
            VarView::Str(text) => s.serialize_str(text),
            VarView::Binary(bytes) => s.serialize_bytes(bytes),
            VarView::Array(items) => {
                self.container::<S::Error>()?;
                let mut seq = s.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(&self.child(item))?;
                }
                seq.end()
            }
            VarView::Object(object) => {
                self.container::<S::Error>()?;
                let properties = object.properties();
                let mut map = s.serialize_map(Some(properties.len()))?;
                for (name, value) in &properties {
                    map.serialize_entry(name.as_str(), &self.child(value))?;
                }
                map.end()
            }
        }
    }
}

/// An `i32` when `n` fits, otherwise an `i64`.
fn integer(n: i64) -> Var {
    i32::try_from(n).map_or(Var::from(n), Var::from)
}

/// Reads a `Var` at a given depth; sets `too_deep` when the depth limit stops it.
#[derive(Clone, Copy)]
pub(crate) struct Seed<'f> {
    depth: usize,
    too_deep: &'f Cell<bool>,
}

impl<'f> Seed<'f> {
    /// A seed for a top-level value.
    pub(crate) fn root(too_deep: &'f Cell<bool>) -> Self {
        Self { depth: 0, too_deep }
    }

    /// A seed for the items of an array or object, or an error past [`MAX_DEPTH`].
    fn nested<E: de::Error>(self) -> Result<Self, E> {
        if self.depth >= MAX_DEPTH {
            self.too_deep.set(true);
            return Err(E::custom(format_args!(
                "input nests more than {MAX_DEPTH} levels deep"
            )));
        }
        Ok(Self {
            depth: self.depth + 1,
            too_deep: self.too_deep,
        })
    }
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Var;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Var, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Var;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null, a bool, a number, a string, bytes, an array or a map")
    }

    fn visit_bool<E>(self, b: bool) -> Result<Var, E> {
        Ok(Var::from(b))
    }

    fn visit_i64<E>(self, n: i64) -> Result<Var, E> {
        Ok(integer(n))
    }

    fn visit_u64<E>(self, n: u64) -> Result<Var, E> {
        Ok(i64::try_from(n).map_or(Var::from(n as f64), integer))
    }

    fn visit_f64<E>(self, x: f64) -> Result<Var, E> {
        Ok(Var::from(x))
    }

    fn visit_str<E>(self, text: &str) -> Result<Var, E> {
        Ok(Var::from(text))
    }

    fn visit_string<E>(self, text: String) -> Result<Var, E> {
        Ok(Var::from(text))
    }

    fn visit_bytes<E>(self, bytes: &[u8]) -> Result<Var, E> {
        Ok(Var::from_bytes(bytes))
    }

    fn visit_byte_buf<E>(self, bytes: Vec<u8>) -> Result<Var, E> {
        Ok(Var::from_bytes(bytes))
    }

    fn visit_unit<E>(self) -> Result<Var, E> {
        Ok(Var::void())
    }

    fn visit_none<E>(self) -> Result<Var, E> {
        Ok(Var::void())
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Var, D::Error> {
        self.deserialize(d)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Var, A::Error> {
        let item = self.nested()?;
        // The size hint is untrusted, so grow as items arrive instead of reserving up front.
        let mut items = Vec::new();
        while let Some(value) = seq.next_element_seed(item)? {
            items.push(value);
        }
        Ok(Var::from(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Var, A::Error> {
        let item = self.nested()?;
        let object = DynamicObject::new();
        while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            let value = map.next_value_seed(item)?;
            object.set_property(Identifier::new(&key), value);
        }
        Ok(Var::from(std::rc::Rc::new(object)))
    }
}

#[cfg(feature = "serde")]
impl Serialize for Var {
    /// Writes the value with the mapping described in [`crate::json`]; fails on values nested
    /// more than [`MAX_DEPTH`] levels deep or containing a cycle.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Depth::root(self).serialize(s)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde_core::Deserialize<'de> for Var {
    /// Reads any self-describing value with the mapping described in [`crate::json`]; fails
    /// on input nested more than [`MAX_DEPTH`] levels deep.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Var, D::Error> {
        let too_deep = Cell::new(false);
        Seed::root(&too_deep).deserialize(d)
    }
}

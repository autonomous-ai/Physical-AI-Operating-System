//! Declared, bounded settings. Values describe saved intent, never proof that
//! another owner applied a change or that a physical capability is ready.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, Write},
};

pub const MAX_ENTRIES: usize = 16;
pub use crate::store::MAX_STATE_BYTES;
/// Leaves space for all current values and sixteen compact retry receipts.
pub const MAX_CATALOG_BYTES: usize = 32_768;
/// The private request is at most 8192 bytes. Its canonical Set envelope is
/// `{"op":"set","mutation":...}`, which contributes another 24 bytes.
pub const MAX_MUTATION_BYTES: usize = 8_192 - 24;
pub const MAX_NAME_BYTES: usize = 64;
pub const MAX_DESCRIPTION_BYTES: usize = 512;
pub const MAX_UNIT_BYTES: usize = 32;
pub const MAX_TEXT_BYTES: usize = 256;
pub const MAX_CHOICES: usize = 16;
pub const MAX_REQUEST_ID_BYTES: usize = 64;

/// Stop serialization at the bound instead of allocating an oversized output
/// and checking afterwards. Fixed struct fields make the encoding stable.
pub(crate) fn json_bytes<T: Serialize>(value: &T, bound: usize) -> Result<Vec<u8>, String> {
    struct Bounded {
        bytes: Vec<u8>,
        bound: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.bound.saturating_sub(self.bytes.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "serialized byte bound",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded {
        bytes: Vec::with_capacity(bound.min(4096)),
        bound,
    };
    serde_json::to_writer(&mut out, value)
        .map_err(|_| "value exceeds its serialized byte bound or cannot be encoded".to_owned())?;
    Ok(out.bytes)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub declarations: Vec<Declaration>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub name: String,
    pub owner: String,
    pub description: String,
    pub unit: String,
    pub rule: Rule,
    pub default: Value,
    pub read_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    // An empty struct variant keeps deny_unknown_fields for this tagged case.
    Bool {},
    Integer {
        min: i64,
        max: i64,
    },
    Number {
        min: f64,
        max: f64,
    },
    Text {
        max_bytes: usize,
        choices: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Value {
    Bool(bool),
    Integer(i64),
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub store_id: String,
    pub revision: u64,
    pub request_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    pub guard: Guard,
    pub changes: Vec<Entry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub store_id: String,
    pub revision: u64,
    pub catalog: Catalog,
    pub values: Vec<Entry>,
}

fn text_fits(value: &str, bound: usize) -> bool {
    value.len() <= bound && !value.chars().any(char::is_control)
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
}

fn name_parts(name: &str) -> Option<(&str, &str)> {
    let (owner, key) = name.split_once('.')?;
    (name.len() <= MAX_NAME_BYTES && token(owner) && token(key)).then_some((owner, key))
}

pub(crate) fn valid_store_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn valid_request_id(value: &str) -> bool {
    (1..=MAX_REQUEST_ID_BYTES).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
}

impl Value {
    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::Number(value) if !value.is_finite() => Err("number must be finite".into()),
            Self::Text(value) if !text_fits(value, MAX_TEXT_BYTES) => {
                Err("text exceeds its bound or contains control characters".into())
            }
            _ => Ok(()),
        }
    }
}

impl Rule {
    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::Bool {} => Ok(()),
            Self::Integer { min, max } if min <= max => Ok(()),
            Self::Number { min, max } if min.is_finite() && max.is_finite() && min <= max => Ok(()),
            Self::Text { max_bytes, choices }
                if (1..=MAX_TEXT_BYTES).contains(max_bytes) && choices.len() <= MAX_CHOICES =>
            {
                let mut unique = BTreeSet::new();
                for choice in choices {
                    if choice.is_empty() || !text_fits(choice, *max_bytes) || !unique.insert(choice)
                    {
                        return Err("text choices must be unique, nonempty and bounded".into());
                    }
                }
                Ok(())
            }
            _ => Err("invalid setting rule or limits".into()),
        }
    }

    pub fn check_value(&self, value: &Value) -> Result<(), String> {
        self.check()?;
        value.check()?;
        let accepted = match (self, value) {
            (Self::Bool {}, Value::Bool(_)) => true,
            (Self::Integer { min, max }, Value::Integer(value)) => value >= min && value <= max,
            (Self::Number { min, max }, Value::Number(value)) => value >= min && value <= max,
            (Self::Text { max_bytes, choices }, Value::Text(value)) => {
                value.len() <= *max_bytes && (choices.is_empty() || choices.contains(value))
            }
            _ => false,
        };
        if accepted {
            Ok(())
        } else {
            Err("value does not match its declared type, choices or limits".into())
        }
    }
}

impl Declaration {
    pub fn check(&self) -> Result<(), String> {
        if name_parts(&self.name).is_none_or(|(owner, _)| owner != self.owner)
            || self.description.trim().is_empty()
            || !text_fits(&self.description, MAX_DESCRIPTION_BYTES)
            || !text_fits(&self.unit, MAX_UNIT_BYTES)
        {
            return Err("invalid declaration name, owner, description or unit".into());
        }
        self.rule.check_value(&self.default)
    }

    pub fn check_value(&self, value: &Value) -> Result<(), String> {
        self.rule.check_value(value)
    }
}

impl Catalog {
    pub fn check(&self) -> Result<(), String> {
        if self.version != 1 || !(1..=MAX_ENTRIES).contains(&self.declarations.len()) {
            return Err("catalog version must be 1 with one to sixteen declarations".into());
        }
        let mut names = BTreeSet::new();
        for declaration in &self.declarations {
            declaration.check()?;
            if !names.insert(&declaration.name) {
                return Err("setting names must be unique".into());
            }
        }
        json_bytes(self, MAX_CATALOG_BYTES).map(|_| ())
    }

    pub fn declaration(&self, name: &str) -> Option<&Declaration> {
        self.declarations.iter().find(|entry| entry.name == name)
    }

    /// Example proposals from existing owner defaults. These declarations do
    /// not connect those owners to Settings or change their effective values.
    pub fn first_three() -> Self {
        Self {
            version: 1,
            declarations: vec![
                Declaration {
                    name: "ears.rise_db".into(),
                    owner: "ears".into(),
                    description: "Desired rise above the learned room floor for a sudden sound; Ears integration is pending.".into(),
                    unit: "dB above room floor".into(),
                    rule: Rule::Number { min: 10.0, max: 60.0 },
                    default: Value::Number(30.0),
                    read_only: false,
                },
                Declaration {
                    name: "light.level".into(),
                    owner: "light".into(),
                    description: "Desired light level from dimmest to brightest; the ring owner still controls application and privacy.".into(),
                    unit: "level".into(),
                    rule: Rule::Integer { min: 1, max: 5 },
                    default: Value::Integer(1),
                    read_only: false,
                },
                Declaration {
                    name: "records.voice_max_age_seconds".into(),
                    owner: "records".into(),
                    description: "Desired age limit for closed Voice metadata sessions; Records integration and cleanup enablement remain separate.".into(),
                    unit: "seconds".into(),
                    rule: Rule::Integer { min: 1, max: 31_622_400 },
                    default: Value::Integer(1_209_600),
                    read_only: false,
                },
            ],
        }
    }
}

impl Guard {
    pub fn check(&self) -> Result<(), String> {
        if !valid_store_id(&self.store_id) || !valid_request_id(&self.request_id) {
            return Err("invalid store or request identity".into());
        }
        Ok(())
    }
}

impl Mutation {
    pub fn check(&self, catalog: &Catalog) -> Result<(), String> {
        self.canonical_bytes(catalog).map(|_| ())
    }

    /// The atomic change set has no ordering. Sort by name before encoding so
    /// equivalent requests have one fingerprint despite JSON property order
    /// or the caller's ordering of distinct settings.
    pub fn canonical_bytes(&self, catalog: &Catalog) -> Result<Vec<u8>, String> {
        self.guard.check()?;
        if !(1..=MAX_ENTRIES).contains(&self.changes.len()) {
            return Err("a mutation must contain one to sixteen changes".into());
        }
        let mut names = BTreeSet::new();
        for entry in &self.changes {
            if !names.insert(&entry.name) {
                return Err("a mutation cannot change the same setting twice".into());
            }
            let declaration = catalog
                .declaration(&entry.name)
                .ok_or("setting is not declared")?;
            if declaration.read_only {
                return Err("setting is read-only".into());
            }
            declaration.check_value(&entry.value)?;
        }
        let mut canonical = self.clone();
        canonical
            .changes
            .sort_by(|left, right| left.name.cmp(&right.name));
        json_bytes(&canonical, MAX_MUTATION_BYTES)
    }
}

impl Snapshot {
    pub fn check(&self) -> Result<(), String> {
        self.catalog.check()?;
        if self.version != 1
            || !valid_store_id(&self.store_id)
            || self.values.len() != self.catalog.declarations.len()
        {
            return Err("invalid snapshot version, identity or value count".into());
        }
        let mut names = BTreeSet::new();
        for entry in &self.values {
            if !names.insert(&entry.name) {
                return Err("snapshot contains duplicate settings".into());
            }
            self.catalog
                .declaration(&entry.name)
                .ok_or("snapshot contains an undeclared setting")?
                .check_value(&entry.value)?;
        }
        json_bytes(self, MAX_STATE_BYTES).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn first_three_have_existing_units_defaults_and_limits() {
        let catalog = Catalog::first_three();
        catalog.check().unwrap();
        assert_eq!(catalog.declarations[0].default, Value::Number(30.0));
        assert_eq!(catalog.declarations[1].default, Value::Integer(1));
        assert_eq!(catalog.declarations[2].default, Value::Integer(1_209_600));
        for (index, low, high) in [
            (0, Value::Number(10.0), Value::Number(60.0)),
            (1, Value::Integer(1), Value::Integer(5)),
            (2, Value::Integer(1), Value::Integer(31_622_400)),
        ] {
            catalog.declarations[index].check_value(&low).unwrap();
            catalog.declarations[index].check_value(&high).unwrap();
        }
    }

    #[test]
    fn numeric_kinds_bounds_and_nonfinite_values_are_exact() {
        let integer = Rule::Integer { min: 1, max: 5 };
        for value in [Value::Integer(0), Value::Integer(6), Value::Number(3.0)] {
            assert!(integer.check_value(&value).is_err());
        }
        let number = Rule::Number {
            min: 10.0,
            max: 60.0,
        };
        for value in [
            Value::Number(f64::NAN),
            Value::Number(f64::INFINITY),
            Value::Number(f64::NEG_INFINITY),
            Value::Number(9.99),
            Value::Number(60.01),
            Value::Integer(30),
        ] {
            assert!(number.check_value(&value).is_err());
        }
        assert!(
            Rule::Number {
                min: f64::NAN,
                max: 1.0
            }
            .check()
            .is_err()
        );
        assert!(Rule::Integer { min: 2, max: 1 }.check().is_err());
    }

    #[test]
    fn text_is_bounded_in_utf8_bytes_and_choices_are_explicit() {
        let text = Rule::Text {
            max_bytes: 4,
            choices: vec![],
        };
        text.check_value(&Value::Text("éé".into())).unwrap();
        assert!(text.check_value(&Value::Text("ééa".into())).is_err());
        assert!(text.check_value(&Value::Text("a\nb".into())).is_err());
        let choices = Rule::Text {
            max_bytes: 4,
            choices: vec!["warm".into(), "blue".into()],
        };
        choices.check_value(&Value::Text("warm".into())).unwrap();
        assert!(choices.check_value(&Value::Text("red".into())).is_err());
        for choices in [
            vec![String::new()],
            vec!["a".into(), "a".into()],
            vec!["abcde".into()],
        ] {
            assert!(
                Rule::Text {
                    max_bytes: 4,
                    choices
                }
                .check()
                .is_err()
            );
        }
    }

    #[test]
    fn invalid_catalogs_fail_without_normalizing_names_or_defaults() {
        let good = Catalog::first_three();
        for name in [
            "ears",
            "Ears.rise_db",
            "ears..rise_db",
            "ears.rise/db",
            "eyes.rise_db",
            "ears.rise_db\n",
        ] {
            let mut catalog = good.clone();
            catalog.declarations[0].name = name.into();
            assert!(catalog.check().is_err());
        }
        let mut catalog = good.clone();
        catalog.declarations.push(catalog.declarations[0].clone());
        assert!(catalog.check().is_err());
        catalog = good.clone();
        catalog.declarations[0].description = "x".repeat(MAX_DESCRIPTION_BYTES + 1);
        assert!(catalog.check().is_err());
        catalog = good.clone();
        catalog.declarations[0].unit = "dB\n".into();
        assert!(catalog.check().is_err());
        catalog = good;
        catalog.declarations[0].default = Value::Number(0.0);
        assert!(catalog.check().is_err());
        assert!(
            Catalog {
                version: 1,
                declarations: vec![]
            }
            .check()
            .is_err()
        );
    }

    #[test]
    fn unknown_fields_duplicate_fields_and_wrong_json_kinds_are_refused() {
        assert!(serde_json::from_value::<Value>(json!({"kind":"integer","value":1.5})).is_err());
        assert!(
            serde_json::from_value::<Value>(json!({"kind":"bool","value":true,"other":0})).is_err()
        );
        assert!(serde_json::from_value::<Rule>(json!({"kind":"bool","other":0})).is_err());
        assert!(
            serde_json::from_str::<Guard>(
                r#"{"store_id":"a","revision":0,"revision":1,"request_id":"b"}"#
            )
            .is_err()
        );
        let mut catalog = serde_json::to_value(Catalog::first_three()).unwrap();
        catalog["unknown"] = true.into();
        assert!(serde_json::from_value::<Catalog>(catalog).is_err());
    }

    #[test]
    fn catalog_admission_reserves_space_for_values_and_full_retry_window() {
        let choices: Vec<String> = (0..16)
            .map(|n| format!("{n:03}{}", "x".repeat(253)))
            .collect();
        let catalog = Catalog {
            version: 1,
            declarations: (0..16)
                .map(|n| Declaration {
                    name: format!("example.value{n}"),
                    owner: "example".into(),
                    description: "x".repeat(512),
                    unit: String::new(),
                    rule: Rule::Text {
                        max_bytes: 256,
                        choices: choices.clone(),
                    },
                    default: Value::Text(choices[0].clone()),
                    read_only: false,
                })
                .collect(),
        };
        for declaration in &catalog.declarations {
            declaration.check().unwrap();
        }
        assert!(catalog.check().is_err());
        assert!(json_bytes(&catalog, MAX_CATALOG_BYTES).is_err());
    }
}

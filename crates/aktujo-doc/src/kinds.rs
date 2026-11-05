// SPDX-License-Identifier: MIT
//! Kinds of documents as data: an identity, a name and fields with their types. A few come with the library; a
//! program defines its own. OPEN: names in other languages than English, kinds defined by users.

use serde::{Deserialize, Serialize};

use crate::{Value, ValueError};

/// The type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// Any text.
    Text,
    /// A day.
    Date,
    /// An amount of money with its currency.
    Money,
    /// A decimal number.
    Number,
    /// Yes or no.
    Flag,
}

/// A field of a kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDef {
    /// The key in events, e.g. `total`.
    pub key: String,
    /// The name people read.
    pub name: String,
    /// The type of its values.
    #[serde(rename = "type")]
    pub field_type: FieldType,
}

/// A kind of document: an invoice, a contract, a notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kind {
    /// The identity in events, e.g. `invoice`.
    pub id: String,
    /// The name people read.
    pub name: String,
    /// Its fields, in the order they are shown.
    pub fields: Vec<FieldDef>,
}

impl Kind {
    /// The field with `key`.
    pub fn field(&self, key: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// Whether `value` is well formed and fits the field `key`. A key the kind does not have takes any value.
    pub fn check(&self, key: &str, value: &Value) -> Result<(), ValueError> {
        value.check()?;
        match self.field(key) {
            Some(field) if field.field_type != value.field_type() => {
                Err(ValueError::Type { key: key.to_owned(), expected: field.field_type })
            }
            _ => Ok(()),
        }
    }
}

fn kind(id: &str, name: &str, fields: &[(&str, &str, FieldType)]) -> Kind {
    Kind {
        id: id.into(),
        name: name.into(),
        fields: fields
            .iter()
            .map(|(key, name, field_type)| FieldDef {
                key: (*key).into(),
                name: (*name).into(),
                field_type: *field_type,
            })
            .collect(),
    }
}

/// The kinds that come with the library.
pub fn builtin() -> Vec<Kind> {
    use FieldType::*;
    vec![
        kind(
            "invoice",
            "Invoice",
            &[
                ("issuer", "Issuer", Text),
                ("number", "Number", Text),
                ("date", "Date", Date),
                ("total", "Total", Money),
                ("due", "Due", Date),
                ("paid", "Paid", Flag),
            ],
        ),
        kind("receipt", "Receipt", &[("issuer", "Issuer", Text), ("date", "Date", Date), ("total", "Total", Money)]),
        kind(
            "contract",
            "Contract",
            &[
                ("party", "Other party", Text),
                ("number", "Number", Text),
                ("start", "Start", Date),
                ("end", "End", Date),
                ("notice", "Notice period", Text),
            ],
        ),
        kind(
            "notice",
            "Official notice",
            &[
                ("authority", "Authority", Text),
                ("reference", "Reference", Text),
                ("date", "Date", Date),
                ("deadline", "Deadline", Date),
                ("amount", "Amount", Money),
            ],
        ),
        kind(
            "payslip",
            "Payslip",
            &[
                ("employer", "Employer", Text),
                ("period", "Period", Text),
                ("gross", "Gross", Money),
                ("net", "Net", Money),
            ],
        ),
        kind(
            "policy",
            "Insurance policy",
            &[
                ("insurer", "Insurer", Text),
                ("number", "Policy number", Text),
                ("premium", "Premium", Money),
                ("start", "Start", Date),
            ],
        ),
        kind("letter", "Letter", &[("sender", "Sender", Text), ("date", "Date", Date), ("subject", "Subject", Text)]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_kinds_are_unique_and_well_named() {
        let kinds = builtin();
        let mut ids: Vec<&str> = kinds.iter().map(|k| k.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), kinds.len());
        for kind in &kinds {
            let mut keys: Vec<&str> = kind.fields.iter().map(|f| f.key.as_str()).collect();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), kind.fields.len(), "{}", kind.id);
        }
    }

    #[test]
    fn a_value_must_fit_its_field() {
        let invoice = builtin().into_iter().find(|k| k.id == "invoice").unwrap();
        assert!(invoice.check("date", &Value::Date("2026-11-05".into())).is_ok());
        assert_eq!(
            invoice.check("date", &Value::Text("5 Nov".into())),
            Err(ValueError::Type { key: "date".into(), expected: FieldType::Date })
        );
        assert!(invoice.check("note", &Value::Text("anything".into())).is_ok());
        assert!(invoice.check("note", &Value::Date("2026-02-30".into())).is_err());
    }
}

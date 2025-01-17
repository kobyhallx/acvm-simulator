//! This module contains vendored code from `noirc_abi` for converting JSON to `InputValue`s.
//! This should be removed in time.

use acvm::FieldElement;
use iter_extended::{btree_map, try_btree_map, try_vecmap, vecmap};
use noirc_abi::{errors::InputParserError, input_parser::InputValue, AbiType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub(super) enum JsonTypes {
    // This is most likely going to be a hex string
    // But it is possible to support UTF-8
    String(String),
    // Just a regular integer, that can fit in 64 bits.
    //
    // The JSON spec does not specify any limit on the size of integer number types,
    // however we restrict the allowable size. Values which do not fit in a u64 should be passed
    // as a string.
    Integer(u64),
    // Simple boolean flag
    Bool(bool),
    // Array of regular integers
    ArrayNum(Vec<u64>),
    // Array of hexadecimal integers
    ArrayString(Vec<String>),
    // Array of booleans
    ArrayBool(Vec<bool>),
    // Struct of JsonTypes
    Table(BTreeMap<String, JsonTypes>),
}

impl From<InputValue> for JsonTypes {
    fn from(value: InputValue) -> Self {
        match value {
            InputValue::Field(f) => {
                let f_str = format!("0x{}", f.to_hex());
                JsonTypes::String(f_str)
            }
            InputValue::Vec(v) => {
                let array = v.iter().map(|i| format!("0x{}", i.to_hex())).collect();
                JsonTypes::ArrayString(array)
            }
            InputValue::String(s) => JsonTypes::String(s),
            InputValue::Struct(map) => {
                let map_with_json_types =
                    btree_map(map, |(key, value)| (key, JsonTypes::from(value)));
                JsonTypes::Table(map_with_json_types)
            }
        }
    }
}

pub(super) fn input_value_from_json_type(
    value: JsonTypes,
    param_type: &AbiType,
    arg_name: &str,
) -> Result<InputValue, InputParserError> {
    let input_value = match value {
        JsonTypes::String(string) => match param_type {
            AbiType::String { .. } => InputValue::String(string),
            AbiType::Field | AbiType::Integer { .. } | AbiType::Boolean => {
                InputValue::Field(parse_str_to_field(&string)?)
            }

            AbiType::Array { .. } | AbiType::Struct { .. } => {
                return Err(InputParserError::AbiTypeMismatch(param_type.clone()))
            }
        },
        JsonTypes::Integer(integer) => {
            let new_value = FieldElement::from(i128::from(integer));

            InputValue::Field(new_value)
        }
        JsonTypes::Bool(boolean) => InputValue::Field(boolean.into()),
        JsonTypes::ArrayNum(arr_num) => {
            let array_elements =
                vecmap(arr_num, |elem_num| FieldElement::from(i128::from(elem_num)));

            InputValue::Vec(array_elements)
        }
        JsonTypes::ArrayString(arr_str) => {
            let array_elements = try_vecmap(arr_str, |elem_str| parse_str_to_field(&elem_str))?;

            InputValue::Vec(array_elements)
        }
        JsonTypes::ArrayBool(arr_bool) => {
            let array_elements = vecmap(arr_bool, FieldElement::from);

            InputValue::Vec(array_elements)
        }

        JsonTypes::Table(table) => match param_type {
            AbiType::Struct { fields } => {
                let native_table = try_btree_map(fields, |(field_name, abi_type)| {
                    // Check that json contains a value for each field of the struct.
                    let field_id = format!("{arg_name}.{field_name}");
                    let value = table
                        .get(field_name)
                        .ok_or_else(|| InputParserError::MissingArgument(field_id.clone()))?;
                    input_value_from_json_type(value.clone(), abi_type, &field_id)
                        .map(|input_value| (field_name.to_string(), input_value))
                })?;

                InputValue::Struct(native_table)
            }
            _ => return Err(InputParserError::AbiTypeMismatch(param_type.clone())),
        },
    };

    Ok(input_value)
}

fn parse_str_to_field(value: &str) -> Result<FieldElement, InputParserError> {
    if value.starts_with("0x") {
        FieldElement::from_hex(value).ok_or_else(|| InputParserError::ParseHexStr(value.to_owned()))
    } else {
        value
            .parse::<i128>()
            .map_err(|err_msg| InputParserError::ParseStr(err_msg.to_string()))
            .map(FieldElement::from)
    }
}

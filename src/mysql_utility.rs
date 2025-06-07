use deserializers::MySqlRowDeserializer;
use serde::de::Error;
use serde::de::{Deserialize, value::Error as DeserializeError};
use sqlx::{
    Decode,
    mysql::{MySqlRow, MySqlValueRef},
};

pub fn from_row<T>(row: MySqlRow) -> Result<T, DeserializeError>
where
    T: for<'de> Deserialize<'de>,
{
    let deserializer = MySqlRowDeserializer::new(&row);
    T::deserialize(deserializer)
}

fn decode_raw_mysql<'a, T>(raw_value: MySqlValueRef<'a>) -> Result<T, DeserializeError>
where
    T: Decode<'a, sqlx::MySql>,
{
    T::decode(raw_value).map_err(|err| {
        DeserializeError::custom(format!(
            "Failed to decode {} value: {:?}",
            std::any::type_name::<T>(),
            err,
        ))
    })
}

mod seq_access {
    use std::fmt::Debug;

    use serde::de::{
        DeserializeSeed, IntoDeserializer, SeqAccess, value::Error as DeserializeError,
    };

    use serde::de::Error as SerdeDeserializeError;

    use sqlx::{Row, TypeInfo, ValueRef, mysql::MySqlValueRef};

    use crate::mysql_utility::{
        decode_raw_mysql,
        deserializers::{MySqlRowDeserializer, MySqlValueDeserializer},
        json::MySqlJson,
    };

    pub(crate) struct MySqlRowSeqAccess<'a> {
        pub(crate) deserializer: MySqlRowDeserializer<'a>,
        pub(crate) num_cols: usize,
    }

    impl<'de, 'a> SeqAccess<'de> for MySqlRowSeqAccess<'a> {
        type Error = DeserializeError;

        fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
        where
            T: DeserializeSeed<'de>,
        {
            if self.deserializer.index < self.num_cols {
                let value = self
                    .deserializer
                    .row
                    .try_get_raw(self.deserializer.index)
                    .map_err(DeserializeError::custom)?;

                let mysql_value_deserializer = MySqlValueDeserializer { value };
                self.deserializer.index += 1;
                seed.deserialize(mysql_value_deserializer).map(Some)
            } else {
                Ok(None)
            }
        }
    }

    pub struct MySqlArraySeqAccess {
        iter: std::vec::IntoIter<serde_json::Value>,
    }

    impl MySqlArraySeqAccess {
        pub fn new<'a>(value_ref: MySqlValueRef<'a>) -> Result<Self, DeserializeError>
        where
            MySqlJson: sqlx::Decode<'a, sqlx::MySql> + Debug,
        {
            let json_value_wrapper: MySqlJson = decode_raw_mysql(value_ref.clone())?;

            if let serde_json::Value::Array(vec) = json_value_wrapper.0 {
                Ok(MySqlArraySeqAccess {
                    iter: vec.into_iter(),
                })
            } else {
                Err(DeserializeError::custom(format!(
                    "Expected a JSON array, got type '{}' (decoded JSON was: {:?})",
                    value_ref.type_info().name(),
                    json_value_wrapper.0
                )))
            }
        }
    }

    impl<'de> SeqAccess<'de> for MySqlArraySeqAccess {
        type Error = DeserializeError;

        fn next_element_seed<U>(&mut self, seed: U) -> Result<Option<U::Value>, Self::Error>
        where
            U: DeserializeSeed<'de>,
        {
            if let Some(json_element) = self.iter.next() {
                seed.deserialize(json_element.into_deserializer())
                    .map_err(DeserializeError::custom)
                    .map(Some)
            } else {
                Ok(None)
            }
        }
    }
}

mod map_access {
    use serde::de::{self, IntoDeserializer, MapAccess, value::Error as DeserializeError};

    use serde::de::Error as SerdeDeserializeError;

    use crate::mysql_utility::deserializers::{MySqlRowDeserializer, MySqlValueDeserializer};
    use sqlx::{Column, Row};

    pub(crate) struct MySqlRowMapAccess<'a> {
        pub(crate) deserializer: MySqlRowDeserializer<'a>,
        pub(crate) num_cols: usize,
    }

    impl<'de, 'a> MapAccess<'de> for MySqlRowMapAccess<'a> {
        type Error = DeserializeError;

        fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
        where
            K: de::DeserializeSeed<'de>,
        {
            if self.deserializer.index < self.num_cols {
                let col_name = self.deserializer.row.columns()[self.deserializer.index].name();
                seed.deserialize(col_name.into_deserializer()).map(Some)
            } else {
                Ok(None)
            }
        }

        fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
        where
            V: de::DeserializeSeed<'de>,
        {
            let value = self
                .deserializer
                .row
                .try_get_raw(self.deserializer.index)
                .map_err(DeserializeError::custom)?;
            let mysql_value_deserializer = MySqlValueDeserializer { value };
            self.deserializer.index += 1;
            seed.deserialize(mysql_value_deserializer)
        }
    }
}

mod deserializers {
    use crate::mysql_utility::{
        decode_raw_mysql,
        json::MySqlJson,
        map_access::MySqlRowMapAccess,
        seq_access::{MySqlArraySeqAccess, MySqlRowSeqAccess},
    };
    use serde::de::Error as SerdeDeserializeError;
    use serde::de::IntoDeserializer;
    use serde::de::{Deserializer, Visitor, value::Error as DeserializeError};
    use serde::forward_to_deserialize_any;
    use sqlx::mysql::{MySqlRow, MySqlValueRef};
    use sqlx::{Row, TypeInfo, ValueRef};

    #[derive(Clone, Copy)]
    pub struct MySqlRowDeserializer<'a> {
        pub(crate) row: &'a MySqlRow,
        pub(crate) index: usize,
    }

    impl<'a> MySqlRowDeserializer<'a> {
        pub fn new(row: &'a MySqlRow) -> Self {
            MySqlRowDeserializer { row, index: 0 }
        }

        #[allow(unused)]
        pub fn is_json(&self) -> bool {
            self.row
                .try_get_raw(0)
                .is_ok_and(|value| value.type_info().name().eq_ignore_ascii_case("JSON"))
        }
    }

    impl<'de, 'a> Deserializer<'de> for MySqlRowDeserializer<'a> {
        type Error = DeserializeError;

        fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let raw_value = self.row.try_get_raw(0).map_err(DeserializeError::custom)?;
            if raw_value.is_null() {
                visitor.visit_none()
            } else {
                visitor.visit_some(self)
            }
        }

        fn deserialize_newtype_struct<V>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_newtype_struct(self)
        }

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match self.row.columns().len() {
                0 => return visitor.visit_unit(),
                1 => {
                    let raw_value_check = self
                        .row
                        .try_get_raw(self.index)
                        .map_err(DeserializeError::custom)?;
                    if raw_value_check
                        .type_info()
                        .name()
                        .eq_ignore_ascii_case("JSON")
                    {
                        let temp_json_val: Result<MySqlJson, _> =
                            decode_raw_mysql(raw_value_check.clone());
                        if let Ok(json_wrapper) = temp_json_val {
                            if matches!(json_wrapper.0, serde_json::Value::Array(_)) {
                                return self.deserialize_seq(visitor);
                            }
                        }
                    }
                }
                _n => {
                    return self.deserialize_seq(visitor);
                }
            };

            let raw_value = self
                .row
                .try_get_raw(self.index)
                .map_err(DeserializeError::custom)?;
            if raw_value.is_null() {
                return visitor.visit_none();
            }
            let deserializer = MySqlValueDeserializer { value: raw_value };
            deserializer.deserialize_any(visitor)
        }

        fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_map(MySqlRowMapAccess {
                deserializer: self,
                num_cols: self.row.columns().len(),
            })
        }

        fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let raw_value = self
                .row
                .try_get_raw(self.index)
                .map_err(DeserializeError::custom)?;

            let type_info = raw_value.type_info();
            let type_name = type_info.name();

            if type_name.eq_ignore_ascii_case("JSON") {
                let json_val_res: Result<MySqlJson, _> = decode_raw_mysql(raw_value.clone());
                if let Ok(json_val) = json_val_res {
                    if matches!(json_val.0, serde_json::Value::Array(_)) {
                        let seq_access = MySqlArraySeqAccess::new(raw_value)?;
                        return visitor.visit_seq(seq_access);
                    }
                }
            }

            let seq_access = MySqlRowSeqAccess {
                deserializer: self,
                num_cols: self.row.columns().len(),
            };
            visitor.visit_seq(seq_access)
        }

        fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.deserialize_seq(visitor)
        }

        fn deserialize_struct<V>(
            self,
            _name: &'static str,
            fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let raw_value = self
                .row
                .try_get_raw(self.index)
                .map_err(DeserializeError::custom)?;
            let type_info = raw_value.type_info();

            if type_info.name().eq_ignore_ascii_case("JSON") {
                let value = decode_raw_mysql::<MySqlJson>(raw_value).map_err(|err| {
                    DeserializeError::custom(format!("Failed to decode JSON: {err}"))
                })?;

                if let serde_json::Value::Object(ref obj) = value.0 {
                    if fields.len() == 1 {
                        if obj.contains_key(fields[0]) {
                            return value.into_deserializer().deserialize_any(visitor);
                        } else {
                            let mut map = serde_json::Map::new();
                            map.insert(fields[0].to_owned(), value.0);
                            return map
                                .into_deserializer()
                                .deserialize_any(visitor)
                                .map_err(DeserializeError::custom);
                        }
                    } else if fields.iter().all(|&field| obj.contains_key(field)) {
                        return value.into_deserializer().deserialize_any(visitor);
                    } else {
                        return Err(DeserializeError::custom(format!(
                            "JSON object missing expected keys: expected {:?}, found keys {:?}",
                            fields,
                            obj.keys().collect::<Vec<_>>()
                        )));
                    }
                } else {
                    return value.into_deserializer().deserialize_any(visitor);
                }
            }
            self.deserialize_map(visitor)
        }

        forward_to_deserialize_any! {
            bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
            bytes byte_buf unit unit_struct
            tuple_struct enum identifier ignored_any
        }
    }

    #[derive(Clone)]
    pub(crate) struct MySqlValueDeserializer<'a> {
        pub(crate) value: MySqlValueRef<'a>,
    }

    impl<'de, 'a> Deserializer<'de> for MySqlValueDeserializer<'a> {
        type Error = DeserializeError;

        fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            if self.value.is_null() {
                visitor.visit_none()
            } else {
                visitor.visit_some(self)
            }
        }

        #[allow(clippy::match_single_binding)]
        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            if self.value.is_null() {
                return visitor.visit_none();
            }
            let type_name_info = self.value.type_info();
            let type_name = type_name_info.name().to_uppercase();

            match type_name.as_str() {
                "FLOAT" => {
                    let v = decode_raw_mysql::<f32>(self.value)?;
                    visitor.visit_f32(v)
                }
                "DOUBLE" | "REAL" => {
                    let v = decode_raw_mysql::<f64>(self.value)?;
                    visitor.visit_f64(v)
                }
                "DECIMAL" | "NUMERIC" | "NEWDECIMAL" => {
                    let numeric = decode_raw_mysql::<rust_decimal::Decimal>(self.value)?;
                    let num: f64 = numeric.try_into().map_err(|_| {
                        DeserializeError::custom(format!(
                            "Failed to parse Decimal {} as f64",
                            numeric
                        ))
                    })?;
                    visitor.visit_f64(num)
                }
                "BIGINT" => {
                    if type_name_info.name().to_lowercase().contains("unsigned") {
                        let v = decode_raw_mysql::<u64>(self.value)?;
                        visitor.visit_u64(v)
                    } else {
                        let v = decode_raw_mysql::<i64>(self.value)?;
                        visitor.visit_i64(v)
                    }
                }
                "INT" | "INTEGER" | "MEDIUMINT" => {
                    if type_name_info.name().to_lowercase().contains("unsigned") {
                        let v = decode_raw_mysql::<u32>(self.value)?;
                        visitor.visit_u32(v)
                    } else {
                        let v = decode_raw_mysql::<i32>(self.value)?;
                        visitor.visit_i32(v)
                    }
                }
                "SMALLINT" => {
                    if type_name_info.name().to_lowercase().contains("unsigned") {
                        let v = decode_raw_mysql::<u16>(self.value)?;
                        visitor.visit_u16(v)
                    } else {
                        let v = decode_raw_mysql::<i16>(self.value)?;
                        visitor.visit_i16(v)
                    }
                }
                "TINYINT" => {
                    if type_name_info.name().eq_ignore_ascii_case("BOOLEAN")
                        || type_name_info.name().eq_ignore_ascii_case("BOOL")
                    {
                        let v = decode_raw_mysql::<bool>(self.value)?;
                        return visitor.visit_bool(v);
                    }

                    match decode_raw_mysql::<bool>(self.value.clone()) {
                        Ok(b) => visitor.visit_bool(b),
                        Err(_) => {
                            if type_name_info.name().to_lowercase().contains("unsigned") {
                                let v = decode_raw_mysql::<u8>(self.value)?;
                                visitor.visit_u8(v)
                            } else {
                                let v = decode_raw_mysql::<i8>(self.value)?;
                                visitor.visit_i8(v)
                            }
                        }
                    }
                }
                "BOOLEAN" | "BOOL" => {
                    let v = decode_raw_mysql::<bool>(self.value)?;
                    visitor.visit_bool(v)
                }
                "DATE" => {
                    let date = decode_raw_mysql::<chrono::NaiveDate>(self.value)?;
                    visitor.visit_string(date.to_string())
                }
                "TIME" => match decode_raw_mysql::<chrono::NaiveTime>(self.value.clone()) {
                    Ok(time) => visitor.visit_string(time.to_string()),
                    Err(_) => Err(DeserializeError::custom(
                        "Failed to decode TIME as NaiveTime",
                    )),
                },
                "DATETIME" => {
                    let ts = decode_raw_mysql::<chrono::NaiveDateTime>(self.value)?;
                    visitor.visit_string(ts.to_string())
                }
                "TIMESTAMP" => {
                    let ts = decode_raw_mysql::<chrono::DateTime<chrono::Utc>>(self.value)?;
                    visitor.visit_string(ts.to_rfc3339())
                }
                "BINARY" | "VARBINARY" => {
                    match decode_raw_mysql::<uuid::Uuid>(self.value.clone()) {
                        Ok(uuid_val) => visitor.visit_string(uuid_val.to_string()),
                        Err(_) => {
                            let bytes = decode_raw_mysql::<Vec<u8>>(self.value)?;
                            visitor.visit_byte_buf(bytes)
                        }
                    }
                }
                "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "GEOMETRY" => {
                    let bytes = decode_raw_mysql::<Vec<u8>>(self.value)?;
                    visitor.visit_byte_buf(bytes)
                }
                "CHAR" | "VARCHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM"
                | "SET" | "YEAR" => {
                    let s = decode_raw_mysql::<String>(self.value)?;
                    visitor.visit_string(s)
                }
                "JSON" => {
                    let value = decode_raw_mysql::<MySqlJson>(self.value)?;
                    value.into_deserializer().deserialize_any(visitor)
                }
                _other => {
                    let as_string =
                        decode_raw_mysql::<String>(self.value.clone()).map_err(|e| {
                            DeserializeError::custom(format!(
                                "Failed to decode unknown type '{}' as String: {}",
                                type_name, e
                            ))
                        })?;
                    visitor.visit_string(as_string)
                }
            }
        }

        forward_to_deserialize_any! {
            bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
            bytes byte_buf unit unit_struct newtype_struct struct
            tuple_struct enum identifier ignored_any tuple seq map
        }
    }
}

mod json {
    use serde::{
        de::{
            self, Deserializer, Error as SerdeDeserializeError, IntoDeserializer,
            value::Error as DeserializeError,
        },
        forward_to_deserialize_any,
    };
    use serde_json::Value;
    use sqlx::{
        MySql,
        mysql::{MySqlTypeInfo, MySqlValueRef},
    };
    use std::fmt::Debug;

    #[derive(Debug)]
    pub(crate) struct MySqlJson(pub(crate) Value);

    impl<'a> sqlx::Decode<'a, MySql> for MySqlJson {
        fn decode(value: MySqlValueRef<'a>) -> Result<Self, sqlx::error::BoxDynError> {
            let s: String = <String as sqlx::Decode<'a, MySql>>::decode(value)?;
            let val: Value = serde_json::from_str(&s)?;
            Ok(MySqlJson(val))
        }
    }

    impl sqlx::Type<MySql> for MySqlJson {
        fn type_info() -> MySqlTypeInfo {
            <sqlx::types::Json<Value> as sqlx::Type<MySql>>::type_info()
        }
        fn compatible(ty: &MySqlTypeInfo) -> bool {
            <sqlx::types::Json<Value> as sqlx::Type<MySql>>::compatible(ty)
        }
    }

    pub struct MySqlJsonDeserializer {
        value: Value,
    }

    impl<'de> Deserializer<'de> for MySqlJsonDeserializer {
        type Error = DeserializeError;

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: de::Visitor<'de>,
        {
            self.value
                .deserialize_any(visitor)
                .map_err(DeserializeError::custom)
        }

        forward_to_deserialize_any! {
            bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct enum identifier ignored_any
        }
    }

    impl<'de> IntoDeserializer<'de, DeserializeError> for MySqlJson {
        type Deserializer = MySqlJsonDeserializer;

        fn into_deserializer(self) -> Self::Deserializer {
            MySqlJsonDeserializer { value: self.0 }
        }
    }
}

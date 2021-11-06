use std::{collections::HashMap, str::FromStr};

use crate::{url_decoder::UrlDecodeError, HttpFailResult};

pub struct QueryString {
    query_string: HashMap<String, String>,
}

impl QueryString {
    pub fn new(src: &str) -> Result<Self, UrlDecodeError> {
        let result = Self {
            query_string: super::url_utils::parse_query_string(src)?,
        };

        Ok(result)
    }

    pub fn get_query_required_string_parameter<'r, 't>(
        &'r self,
        name: &'t str,
    ) -> Result<&'r String, HttpFailResult> {
        let result = self.query_string.get(name);

        match result {
            Some(e) => Ok(e),
            None => Err(HttpFailResult::as_query_parameter_required(name)),
        }
    }

    pub fn get_query_optional_string_parameter<'r, 't>(
        &'r self,
        name: &'t str,
    ) -> Option<&'r String> {
        return self.query_string.get(name);
    }

    pub fn get_query_bool_parameter<'r, 't>(&'r self, name: &'t str, default_value: bool) -> bool {
        let result = self.query_string.get(name);

        match result {
            Some(value) => {
                if value == "1" || value.to_lowercase() == "true" {
                    return true;
                }

                return false;
            }
            None => return default_value,
        };
    }

    pub fn get_query_optional_parameter<'r, 't, T: FromStr>(&'r self, name: &'t str) -> Option<T> {
        let result = self.query_string.get(name);

        match result {
            Some(value) => {
                let result = value.parse::<T>();

                return match result {
                    Ok(value) => Some(value),
                    _ => None,
                };
            }
            None => return None,
        };
    }

    pub fn get_query_required_parameter<'r, 't, T: FromStr>(
        &'r self,
        name: &'t str,
    ) -> Result<T, HttpFailResult> {
        let result = self.query_string.get(name);

        match result {
            Some(value) => {
                let result = value.parse::<T>();

                return match result {
                    Ok(value) => Ok(value),
                    _ => Err(HttpFailResult::as_query_parameter_required(name)),
                };
            }
            None => return Err(HttpFailResult::as_query_parameter_required(name)),
        };
    }
}

use crate::url_decoder::UrlDecodeError;

use super::{ReadingEncodedDataError, UrlEncodedValue};

pub struct UrlEncodedDataReader<'s> {
    src: &'s str,
    query_string: Vec<UrlEncodedValue<'s>>,
}

impl<'s> UrlEncodedDataReader<'s> {
    pub fn new(src: &'s str) -> Result<Self, UrlDecodeError> {
        // Single shared parser (see crate::query_string::parse): decodes keys, keeps
        // flag-style params, skips empty segments.
        let query_string = crate::query_string::parse(src)?;

        Ok(Self { query_string, src })
    }

    pub fn get_required(
        &'s self,
        name: &str,
    ) -> Result<UrlEncodedValue<'s>, ReadingEncodedDataError> {
        let result = self.get_optional(name);

        match result {
            Some(e) => Ok(e),
            None => Err(ReadingEncodedDataError::RequiredParameterIsMissing(
                name.to_string(),
            )),
        }
    }

    pub fn get_optional(&'s self, name: &str) -> Option<UrlEncodedValue<'s>> {
        for itm in &self.query_string {
            if itm.get_name() == name {
                return Some(itm.clone());
            }
        }
        None
    }

    pub fn get_vec(&'s self, name: &str) -> Vec<UrlEncodedValue<'s>> {
        let mut result = Vec::new();
        for itm in &self.query_string {
            if itm.get_name() == name {
                result.push(itm.clone());
            }
        }

        result
    }

    pub fn get_raw(&self) -> &str {
        self.src
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_basic() {
        let query_string =
            "tableName=deposit-restrictions&partitionKey=%2A&rowKey=1abfc&field=1a+bfc";

        let query_string = UrlEncodedDataReader::new(query_string).unwrap();

        let result = query_string
            .get_optional("partitionKey")
            .unwrap()
            .as_string()
            .unwrap();

        assert_eq!("*", result);

        let result = query_string
            .get_optional("rowKey")
            .unwrap()
            .as_string()
            .unwrap();

        assert_eq!("1abfc", result);

        let result = query_string
            .get_optional("field")
            .unwrap()
            .as_string()
            .unwrap();

        assert_eq!("1a bfc", result);
    }

    #[test]
    pub fn test_vec() {
        let query_string =
            "tableName=deposit-restrictions&param[]=1&param[]=2&param[]=3&param[]=4&param[]=5";

        let query_string = UrlEncodedDataReader::new(query_string).unwrap();

        let mut result = Vec::new();

        for itm in query_string.get_vec("param") {
            result.push(itm.as_string().unwrap());
        }

        assert_eq!(vec!["1", "2", "3", "4", "5"], result);
    }

    #[test]
    pub fn test_vec_of_usize() {
        let query_string =
            "tableName=deposit-restrictions&param[]=1&param[]=2&param[]=3&param[]=4&param[]=5&prm[]=1&prm[]=2&prm[]=3&prm[]=4";

        let query_string = UrlEncodedDataReader::new(query_string).unwrap();

        let mut result: Vec<usize> = Vec::new();

        for itm in query_string.get_vec("param") {
            result.push(itm.parse().unwrap());
        }

        assert_eq!(vec![1, 2, 3, 4, 5], result);

        let mut result: Vec<i32> = Vec::new();

        for itm in query_string.get_vec("prm") {
            result.push(itm.parse().unwrap());
        }

        assert_eq!(vec![1, 2, 3, 4], result);

        let mut result: Vec<i32> = Vec::new();

        for itm in query_string.get_vec("params") {
            result.push(itm.parse().unwrap());
        }

        assert_eq!(0, result.len());
    }

    #[test]
    pub fn test_flag_param_is_visible() {
        // Regression: '='-less params used to be dropped, so get_required reported
        // them as missing.
        let reader = UrlEncodedDataReader::new("verbose&name=John").unwrap();
        assert!(reader.get_optional("verbose").is_some());
        assert!(reader.get_required("verbose").is_ok());
        assert_eq!(reader.get_optional("name").unwrap().as_string().unwrap(), "John");
    }

    #[test]
    pub fn test_parse_decodes_before_parsing() {
        // "%2D5" decodes to "-5"; parse used to run on the raw escaped form.
        let reader = UrlEncodedDataReader::new("n=%2D5").unwrap();
        let value: i32 = reader.get_required("n").unwrap().parse().unwrap();
        assert_eq!(value, -5);
    }
}

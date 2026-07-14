use super::HttpResult;

pub trait IntoHttpResult {
    fn into_http_result_object(
        self,
        http_code: u16,
        nullable: bool,
        description: &str,
    ) -> HttpResult;
    fn into_http_result_array(
        self,
        http_code: u16,
        nullable: bool,
        description: &str,
    ) -> HttpResult;
}

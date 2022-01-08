use crate::{HttpContext, HttpFailResult, HttpOkResult};
use async_trait::async_trait;
pub enum MiddleWareResult {
    Ok(HttpOkResult),
    Next(HttpContext),
}
#[async_trait]
pub trait HttpServerMiddleware {
    async fn handle_request(&self, ctx: HttpContext) -> Result<MiddleWareResult, HttpFailResult>;
}

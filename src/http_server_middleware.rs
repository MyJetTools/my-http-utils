use crate::{HttpContext, HttpFailResult, HttpOkResult};
use async_trait::async_trait;
use std::sync::Arc;
pub enum MiddleWareResult {
    Ok(HttpOkResult),
    Next(HttpContext),
}
#[async_trait]
pub trait HttpServerMiddleware<TAppContext: Send + Sync + 'static> {
    async fn handle_request(
        &self,
        ctx: HttpContext,
        app: &Arc<TAppContext>,
    ) -> Result<MiddleWareResult, HttpFailResult>;
}

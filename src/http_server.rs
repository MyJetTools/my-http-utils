use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use rust_extensions::ApplicationStates;
use std::{net::SocketAddr, time::Duration};

use std::sync::Arc;

use crate::{HttpContext, HttpFailResult, HttpServerMiddleware};

pub struct HttpServerData<TAppContext: Send + Sync + 'static> {
    app: Arc<TAppContext>,
    middlewares: Vec<Arc<dyn HttpServerMiddleware<TAppContext> + Send + Sync + 'static>>,
}

pub struct MyHttpServer<TAppContext: Send + Sync + 'static> {
    pub addr: SocketAddr,
    pub app: Arc<TAppContext>,
    middlewares: Vec<Arc<dyn HttpServerMiddleware<TAppContext> + Send + Sync + 'static>>,
}

impl<TAppContext: Send + Sync + 'static> MyHttpServer<TAppContext> {
    pub fn new(addr: SocketAddr, app: Arc<TAppContext>) -> Self {
        Self {
            app,
            addr,
            middlewares: Vec::new(),
        }
    }
    pub fn add_middleware(
        &mut self,
        middleware: Arc<dyn HttpServerMiddleware<TAppContext> + Send + Sync + 'static>,
    ) {
        self.middlewares.push(middleware);
    }

    pub fn start<TAppStates>(&self, app_states: Arc<TAppStates>)
    where
        TAppContext: Send + Sync + 'static,
        TAppStates: ApplicationStates + Send + Sync + 'static,
    {
        let http_server_data = HttpServerData {
            app: self.app.clone(),
            middlewares: self.middlewares.clone(),
        };

        tokio::spawn(start(
            self.addr.clone(),
            Arc::new(http_server_data),
            app_states,
        ));
    }
}

pub async fn start<TAppContext, TAppStates>(
    addr: SocketAddr,
    http_server_data: Arc<HttpServerData<TAppContext>>,
    app_states: Arc<TAppStates>,
) where
    TAppContext: Send + Sync + 'static,
    TAppStates: ApplicationStates + Send + Sync + 'static,
{
    let http_server_data_spawned = http_server_data.clone();

    let make_service = make_service_fn(move |conn: &AddrStream| {
        let http_server_data = http_server_data_spawned.clone();
        let addr = conn.remote_addr();

        async move {
            Ok::<_, hyper::Error>(service_fn(move |_req| {
                handle_requests(_req, http_server_data.clone(), addr)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_service);

    let server = server.with_graceful_shutdown(shutdown_signal(app_states));

    if let Err(e) = server.await {
        eprintln!("Http Server error: {}", e);
    }
}

pub async fn handle_requests<TAppContext: Send + Sync + 'static>(
    req: Request<Body>,
    http_server_data: Arc<HttpServerData<TAppContext>>,
    addr: SocketAddr,
) -> hyper::Result<Response<Body>> {
    let mut ctx = HttpContext::new(req, addr);

    for middleware in &http_server_data.middlewares {
        match middleware.handle_request(ctx, &http_server_data.app).await {
            Ok(result) => match result {
                crate::MiddleWareResult::Ok(ok_result) => {
                    return Ok(ok_result.into());
                }
                crate::MiddleWareResult::Next(next_ctx) => {
                    ctx = next_ctx;
                }
            },
            Err(fail_result) => {
                return Ok(fail_result.into());
            }
        }
    }

    let not_found = HttpFailResult::as_not_found("Page not found".to_string(), false);

    return Ok(not_found.into());
}

async fn shutdown_signal<TAppStates: ApplicationStates>(app: Arc<TAppStates>) {
    let duration = Duration::from_secs(1);
    while !app.is_shutting_down() {
        tokio::time::sleep(duration).await;
    }
}

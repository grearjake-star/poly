use anyhow::Result;
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use prometheus::{Encoder, Registry, TextEncoder};
use std::net::SocketAddr;
use tracing::info;

#[derive(Clone)]
pub struct MetricsHandle {
    registry: Registry,
}

impl MetricsHandle {
    pub fn new() -> Self {
        Self {
            registry: Registry::new(),
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<()> {
        let registry = self.registry.clone();
        let make_svc = make_service_fn(move |_| {
            let registry = registry.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |_req: Request<Body>| {
                    let registry = registry.clone();
                    async move {
                        let encoder = TextEncoder::new();
                        let metric_families = registry.gather();
                        let mut buffer = Vec::new();
                        encoder.encode(&metric_families, &mut buffer).unwrap();
                        Ok::<_, hyper::Error>(
                            Response::builder()
                                .status(200)
                                .header("Content-Type", encoder.format_type())
                                .body(Body::from(buffer))
                                .unwrap(),
                        )
                    }
                }))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);
        info!("metrics exporter listening", %addr);
        server.await?;
        Ok(())
    }
}

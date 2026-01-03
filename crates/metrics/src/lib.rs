use anyhow::Result;
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use prometheus::{Counter, Encoder, Registry, TextEncoder};
use std::net::SocketAddr;
use tracing::info;

#[derive(Clone)]
pub struct MetricsHandle {
    registry: Registry,
    heartbeat_counter: Counter,
}

impl Default for MetricsHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsHandle {
    pub fn new() -> Self {
        let registry = Registry::new();
        let heartbeat_counter =
            Counter::new("heartbeat_total", "Number of heartbeat ticks since startup")
                .expect("heartbeat counter should be valid");
        registry
            .register(Box::new(heartbeat_counter.clone()))
            .expect("heartbeat counter should register");

        Self {
            registry,
            heartbeat_counter,
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn heartbeat_counter(&self) -> Counter {
        self.heartbeat_counter.clone()
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
        info!(%addr, "metrics exporter listening");
        server.await?;
        Ok(())
    }
}

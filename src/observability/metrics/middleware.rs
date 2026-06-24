use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use axum::{
    body::Body,
    extract::MatchedPath,
    http::{Request, Response},
};
use tower::{Layer, Service};

use super::{REQUEST_COUNT, REQUEST_DURATION};

#[derive(Debug, Clone, Copy)]
pub struct HttpRequestMetricsLayer;

impl<S> Layer<S> for HttpRequestMetricsLayer {
    type Service = HttpRequestMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpRequestMetricsService { inner }
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequestMetricsService<S> {
    inner: S,
}

impl<S, B> Service<Request<B>> for HttpRequestMetricsService<S>
where
    S: Service<Request<B>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let start = Instant::now();
        let method = req.method().to_string();
        let route = req
            .extensions()
            .get::<MatchedPath>()
            .map(|p| p.as_str().to_owned())
            .unwrap_or_else(|| "UNKNOWN".to_string());

        REQUEST_COUNT.add(
            1,
            crate::metric_attributes!(
                ("http.request.method", method.clone()),
                ("http.route", route.clone()),
            ),
        );

        let future = self.inner.call(req);

        Box::pin(async move {
            let response = future.await?;
            let status = response.status().as_u16();
            let duration = start.elapsed();

            REQUEST_DURATION.record(
                duration.as_secs_f64(),
                crate::metric_attributes!(
                    ("http.request.method", method.clone()),
                    ("http.route", route.clone()),
                    ("http.response.status_code", status.to_string()),
                ),
            );

            Ok(response)
        })
    }
}

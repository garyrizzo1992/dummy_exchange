//! Shared OTLP tracing and W3C context propagation. No request bodies or secrets are recorded.
use opentelemetry::{
    Context, KeyValue, global,
    trace::{TraceContextExt, TracerProvider},
};
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider};
use std::{collections::HashMap, env, time::Duration};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub struct TelemetryGuard(Option<SdkTracerProvider>);
impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = &self.0 {
            let _ = provider.shutdown();
        }
    }
}
pub fn init(service: &str) -> anyhow::Result<TelemetryGuard> {
    global::set_text_map_propagator(TraceContextPropagator::new());
    let provider = if let Ok(endpoint) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(format!("{}/v1/traces", endpoint.trim_end_matches('/')))
            .with_protocol(Protocol::HttpBinary)
            .with_timeout(Duration::from_secs(3))
            .build()?;
        Some(
            SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(
                    Resource::builder()
                        .with_attributes([KeyValue::new("service.name", service.to_owned())])
                        .build(),
                )
                .build(),
        )
    } else {
        None
    };
    let layer = provider
        .as_ref()
        .map(|p| tracing_opentelemetry::layer().with_tracer(p.tracer("dummy-exchange")));
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(layer)
        .with(tracing_subscriber::fmt::layer().json())
        .try_init()?;
    Ok(TelemetryGuard(provider))
}
pub fn inject() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    global::get_text_map_propagator(|p| {
        p.inject_context(&tracing::Span::current().context(), &mut headers)
    });
    headers
}
pub fn extract(headers: &HashMap<String, String>) -> Context {
    global::get_text_map_propagator(|p| p.extract(headers))
}

pub fn set_parent(span: &tracing::Span, headers: &HashMap<String, String>) {
    let _ = span.set_parent(extract(headers));
    span.record(
        "trace_id",
        span.context().span().span_context().trace_id().to_string(),
    );
}

pub fn add_link(span: &tracing::Span, headers: &HashMap<String, String>) {
    let parent = extract(headers);
    let context = parent.span().span_context().clone();
    if context.is_valid() {
        span.add_link(context);
    }
}

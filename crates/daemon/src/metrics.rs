//! Prometheus metrics registry.

use anyhow::Result;
use metrics::{describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

pub(crate) const TRANSFERS_STARTED_TOTAL: &str = "lsi_transfers_started_total";
pub(crate) const TRANSFERS_COMPLETED_TOTAL: &str = "lsi_transfers_completed_total";
pub(crate) const TRANSFER_BYTES_TOTAL: &str = "lsi_transfer_bytes_total";
pub(crate) const HOOK_DELIVERY_FAILURES_TOTAL: &str = "lsi_hook_delivery_failures_total";
pub(crate) const PEERS_TRUSTED: &str = "lsi_peers_trusted";

#[cfg(test)]
pub(crate) const METRIC_NAMES: &[&str] = &[
    TRANSFERS_STARTED_TOTAL,
    TRANSFERS_COMPLETED_TOTAL,
    TRANSFER_BYTES_TOTAL,
    HOOK_DELIVERY_FAILURES_TOTAL,
    PEERS_TRUSTED,
];

#[allow(dead_code)]
pub(crate) struct MetricsRuntime {
    pub(crate) handle: PrometheusHandle,
}

pub(crate) fn install() -> Result<MetricsRuntime> {
    describe_metrics();
    let handle = PrometheusBuilder::new().install_recorder()?;
    Ok(MetricsRuntime { handle })
}

fn describe_metrics() {
    describe_counter!(TRANSFERS_STARTED_TOTAL, "Transfers started by the daemon.");
    describe_counter!(TRANSFERS_COMPLETED_TOTAL, "Transfers completed by the daemon.");
    describe_histogram!(TRANSFER_BYTES_TOTAL, "Bytes transferred by the daemon.");
    describe_counter!(HOOK_DELIVERY_FAILURES_TOTAL, "Hook delivery failures.");
    describe_gauge!(PEERS_TRUSTED, "Trusted peers known by the daemon.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_names_include_operational_counters() {
        assert!(METRIC_NAMES.contains(&"lsi_transfers_started_total"));
        assert!(METRIC_NAMES.contains(&"lsi_transfers_completed_total"));
        assert!(METRIC_NAMES.contains(&"lsi_transfer_bytes_total"));
        assert!(METRIC_NAMES.contains(&"lsi_hook_delivery_failures_total"));
        assert!(METRIC_NAMES.contains(&"lsi_peers_trusted"));
    }

    #[test]
    fn metric_names_are_stable_and_unique() {
        let mut names = METRIC_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), METRIC_NAMES.len());
    }
}

use crate::state::{
    HttpBotDetail, HttpBotPnlStatus, HttpBotPollingStatus, HttpBotSummary, HttpReconciliationReport,
};
use openticker_core::Timeframe;
use openticker_dataplane::StreamStatus;
use openticker_runtime::InstanceSummary as RuntimeInstanceSummary;
use std::collections::HashMap;

pub(super) fn stream_status_map(streams: Vec<StreamStatus>) -> HashMap<String, StreamStatus> {
    streams
        .into_iter()
        .map(|stream| {
            (
                stream_key_id(
                    &stream.key.account_id,
                    &stream.key.symbol,
                    stream.key.timeframe,
                ),
                stream,
            )
        })
        .collect()
}

pub(super) fn stream_key_id(account_id: &str, symbol: &str, timeframe: Timeframe) -> String {
    format!("{account_id}/{symbol}/{timeframe}")
}

#[allow(clippy::too_many_lines)]
pub(super) fn bot_summary_view(
    summary: RuntimeInstanceSummary,
    stream_statuses: &HashMap<String, StreamStatus>,
    lane_summaries: Option<&[openticker_runtime::LaneSummary]>,
) -> HttpBotSummary {
    let polling_status = stream_statuses.get(&stream_key_id(
        &summary.account,
        &summary.symbol,
        summary.timeframe,
    ));
    let summary_mark_price = polling_status
        .and_then(|status| status.latest_bar.as_ref())
        .map(|bar| bar.close);
    let summary_mark_timestamp = polling_status
        .and_then(|status| status.latest_bar.as_ref())
        .map(|bar| bar.timestamp.to_rfc3339());
    let mut mark_price = summary_mark_price;
    let mut mark_timestamp = summary_mark_timestamp;

    let unrealized_usd = if let Some(lanes) = lane_summaries {
        let open_lanes = lanes
            .iter()
            .filter(|lane| {
                lane.has_position && lane.quantity.is_finite() && lane.quantity > f64::EPSILON
            })
            .collect::<Vec<_>>();

        if open_lanes.is_empty() {
            Some(0.0)
        } else {
            let mut lane_marks = Vec::with_capacity(open_lanes.len());
            let mut total_unrealized_usd = 0.0;
            let mut missing_lane_mark = false;

            for lane in open_lanes {
                let lane_entry_price = lane.entry_price.filter(|price| price.is_finite());
                let lane_mark = stream_statuses
                    .get(&stream_key_id(
                        &summary.account,
                        &lane.symbol,
                        summary.timeframe,
                    ))
                    .and_then(|status| status.latest_bar.as_ref())
                    .map(|bar| (bar.close, bar.timestamp.to_rfc3339()));

                match (lane_entry_price, lane_mark) {
                    (Some(entry_price), Some((mark, timestamp))) if mark.is_finite() => {
                        total_unrealized_usd += (mark - entry_price) * lane.quantity;
                        lane_marks.push((mark, timestamp));
                    }
                    _ => {
                        missing_lane_mark = true;
                        break;
                    }
                }
            }

            if missing_lane_mark {
                mark_price = None;
                mark_timestamp = None;
                None
            } else {
                if lane_marks.len() == 1 {
                    mark_price = Some(lane_marks[0].0);
                    mark_timestamp = Some(lane_marks[0].1.clone());
                } else {
                    mark_price = None;
                    mark_timestamp = None;
                }

                Some(total_unrealized_usd)
            }
        }
    } else if summary.position.has_position
        && summary.position.quantity > f64::EPSILON
        && summary.position.entry_price.is_some()
    {
        match (summary.position.entry_price, summary_mark_price) {
            (Some(entry_price), Some(mark)) if entry_price.is_finite() && mark.is_finite() => {
                Some((mark - entry_price) * summary.position.quantity)
            }
            _ => None,
        }
    } else {
        Some(0.0)
    };
    let total_usd = unrealized_usd.map(|unrealized_usd| summary.pnl.realized_usd + unrealized_usd);

    HttpBotSummary {
        id: summary.id,
        enabled: summary.enabled,
        market: summary.market,
        symbols: summary.symbols,
        symbol: summary.symbol,
        timeframe: summary.timeframe,
        account: summary.account,
        execution_mode: summary.execution_mode,
        live_mode_active: summary.live_mode_active,
        mode_banner: summary.mode_banner,
        state: summary.state,
        position: summary.position,
        pnl: HttpBotPnlStatus {
            realized_usd: summary.pnl.realized_usd,
            unrealized_usd,
            total_usd,
            mark_price,
            mark_timestamp,
        },
        open_symbol_count: summary.open_symbol_count,
        aggregate_position_notional_usd: summary.aggregate_position_notional_usd,
        aggregate_realized_pnl_usd: summary.aggregate_realized_pnl_usd,
        reconciliation_blocked: summary.reconciliation_blocked,
        reconciliation_by_symbol: summary.reconciliation_by_symbol,
        warmup: summary.warmup,
        warmup_ready_symbols: summary.warmup_ready_symbols,
        polling: HttpBotPollingStatus {
            enabled: summary.polling_enabled,
            interval_ms: summary.polling_interval_ms,
            last_attempt_ms: polling_status.and_then(|status| status.last_attempt_ms),
            last_success_ms: polling_status.and_then(|status| status.last_success_ms),
            last_error: polling_status.and_then(|status| status.last_error.clone()),
            last_polled_bar_timestamp: polling_status
                .and_then(|status| status.latest_bar.as_ref())
                .map(|bar| bar.timestamp.to_rfc3339()),
            last_polled_bar_close: polling_status
                .and_then(|status| status.latest_bar.as_ref())
                .map(|bar| bar.close),
        },
    }
}

pub(super) fn bot_detail_view(
    summary: RuntimeInstanceSummary,
    lanes: Vec<openticker_runtime::LaneSummary>,
    stream_statuses: &HashMap<String, StreamStatus>,
) -> HttpBotDetail {
    let bot = bot_summary_view(summary, stream_statuses, Some(lanes.as_slice()));
    HttpBotDetail { bot, lanes }
}

pub(super) fn reconciliation_report_view(
    report: openticker_runtime::ReconciliationReport,
    lane_summaries: &[openticker_runtime::LaneSummary],
    stream_statuses: &HashMap<String, StreamStatus>,
) -> HttpReconciliationReport {
    HttpReconciliationReport {
        bot: bot_summary_view(report.instance, stream_statuses, Some(lane_summaries)),
        latest: report.latest,
        lanes: report.lanes,
    }
}

use crate::{LaneRuntime, StreamKey};

pub(crate) fn instance_matches_stream_key(instance: &LaneRuntime, key: &StreamKey) -> bool {
    instance.config.account == key.account_id
        && instance.config.timeframe == key.timeframe
        && instance.lane_symbol == key.symbol
}

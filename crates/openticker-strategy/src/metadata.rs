use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorSignal, IndicatorSignalMetadataFilters, SignalMetadata,
    SignalStrength,
};

pub(crate) fn metadata_filter_block_reason(
    capabilities: IndicatorMetadataCapabilities,
    filters: &IndicatorSignalMetadataFilters,
    metadata: &SignalMetadata,
    signal: IndicatorSignal,
) -> Option<String> {
    let filter = match signal {
        IndicatorSignal::BuyPreview | IndicatorSignal::BuyConfirmed => &filters.entry,
        IndicatorSignal::SellPreview | IndicatorSignal::SellConfirmed => &filters.exit,
        IndicatorSignal::None => return None,
    };

    if !filter.is_configured() {
        return None;
    }

    if !filter.allowed_strengths.is_empty() && capabilities.supports_strength {
        let strength = metadata.strength?;
        if !filter.allowed_strengths.contains(&strength) {
            return Some(format!("strength={} not allowed", strength_label(strength)));
        }
    }

    if !filter.allowed_reason_codes.is_empty() && capabilities.supports_reason_code {
        let reason_code = metadata.reason_code.as_deref()?;
        if !filter
            .allowed_reason_codes
            .iter()
            .any(|value| value == reason_code)
        {
            return Some(format!("reason_code={reason_code} not allowed"));
        }
    }

    if capabilities.supports_tags {
        if let Some(blocked_tag) = filter
            .blocked_tags
            .iter()
            .find(|tag| metadata.tags.iter().any(|value| value == *tag))
        {
            return Some(format!("blocked_tag={blocked_tag}"));
        }

        if let Some(missing_tag) = filter
            .required_tags_all
            .iter()
            .find(|tag| !metadata.tags.iter().any(|value| value == *tag))
        {
            return Some(format!("missing_required_tag={missing_tag}"));
        }

        if !filter.required_tags_any.is_empty()
            && !filter
                .required_tags_any
                .iter()
                .any(|tag| metadata.tags.iter().any(|value| value == tag))
        {
            return Some("missing_any_required_tag".to_owned());
        }
    }

    None
}

pub(crate) fn signal_label(signal: IndicatorSignal) -> &'static str {
    match signal {
        IndicatorSignal::None => "none",
        IndicatorSignal::BuyPreview => "buy_preview",
        IndicatorSignal::BuyConfirmed => "buy_confirmed",
        IndicatorSignal::SellPreview => "sell_preview",
        IndicatorSignal::SellConfirmed => "sell_confirmed",
    }
}

fn strength_label(strength: SignalStrength) -> &'static str {
    match strength {
        SignalStrength::Normal => "normal",
        SignalStrength::Strong => "strong",
    }
}

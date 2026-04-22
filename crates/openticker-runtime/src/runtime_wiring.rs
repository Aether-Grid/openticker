#[cfg(test)]
mod tests {
    use crate::test_support::{fixture_bundle, test_bar};
    use openticker_config::IndicatorInstanceConfig;
    use openticker_core::{
        IndicatorRole, IndicatorSignalMetadataFilters, IndicatorSignalPolicy, SignalPhase,
    };
    use toml::Table;

    #[test]
    fn runtime_instantiates_indicator_from_config_type() {
        let mut config = fixture_bundle();
        config.instances[0].indicators[0].indicator_type = "rsi_threshold".to_owned();

        let mut runtime = crate::Runtime::from_config(&config);
        runtime.start_instance("aapl").unwrap();

        let outcome = runtime
            .process_bar("aapl", &test_bar(100.0), SignalPhase::Confirmed)
            .unwrap();
        assert_eq!(outcome.instance_id, "aapl");
    }

    #[test]
    fn runtime_supports_consensus_strategy_with_multiple_indicators() {
        let mut config = fixture_bundle();
        config.instances[0].strategy = "consensus".to_owned();
        config.instances[0].indicators = vec![
            IndicatorInstanceConfig {
                id: "primary-1".to_owned(),
                indicator_type: "sma_crossover".to_owned(),
                enabled: true,
                role: Some(IndicatorRole::PrimarySignal),
                signal_policy: Some(IndicatorSignalPolicy::ConfirmedRequired),
                weight: Some(1.0),
                metadata_filters: IndicatorSignalMetadataFilters::default(),
                params: Table::new(),
            },
            IndicatorInstanceConfig {
                id: "filter-1".to_owned(),
                indicator_type: "rsi_threshold".to_owned(),
                enabled: true,
                role: Some(IndicatorRole::Filter),
                signal_policy: Some(IndicatorSignalPolicy::ConfirmedRequired),
                weight: Some(1.0),
                metadata_filters: IndicatorSignalMetadataFilters::default(),
                params: Table::from_iter([
                    ("period".to_owned(), toml::Value::Integer(14)),
                    ("oversold".to_owned(), toml::Value::Float(30.0)),
                    ("overbought".to_owned(), toml::Value::Float(70.0)),
                ]),
            },
        ];

        let mut runtime = crate::Runtime::from_config(&config);
        runtime.start_instance("aapl").unwrap();

        let outcome = runtime
            .process_bar("aapl", &test_bar(100.0), SignalPhase::Confirmed)
            .unwrap();
        assert_eq!(outcome.instance_id, "aapl");
    }
}

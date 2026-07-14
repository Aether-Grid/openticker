use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalPhase {
    Preview,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossType {
    None,
    CrossOver,
    CrossUnder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorSignal {
    None,
    BuyPreview,
    BuyConfirmed,
    SellPreview,
    SellConfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalStrength {
    Normal,
    Strong,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct IndicatorTradeLevels {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trailing_stop: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IndicatorFactValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl From<bool> for IndicatorFactValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i8> for IndicatorFactValue {
    fn from(value: i8) -> Self {
        Self::Int(i64::from(value))
    }
}

/// Converts a `usize` count/length into an integer fact.
///
/// The conversion is exact for all realistic indicator values. Values above
/// `i64::MAX` (~9.2e18) saturate to `i64::MAX`; this boundary is unreachable
/// for indicator facts (counts, periods, and window lengths), and a
/// `debug_assert!` guards against it in debug builds.
impl From<usize> for IndicatorFactValue {
    fn from(value: usize) -> Self {
        // Indicator facts carry small counts/lengths/periods (pattern counts,
        // moving-average windows, etc.), so this conversion is exact for every
        // realistic value: `usize` only exceeds `i64::MAX` (~9.2e18) on
        // 64-bit-and-wider platforms with absurd magnitudes that no indicator
        // produces. Such an out-of-range value saturates to `i64::MAX` rather
        // than wrapping or panicking; the `debug_assert!` flags it in debug
        // builds so a real overflow is caught during testing.
        debug_assert!(
            i64::try_from(value).is_ok(),
            "IndicatorFactValue::from(usize) saturated: {value} exceeds i64::MAX",
        );
        Self::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl From<i64> for IndicatorFactValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for IndicatorFactValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<String> for IndicatorFactValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for IndicatorFactValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct IndicatorMetadataCapabilities {
    #[serde(default)]
    pub supports_strength: bool,
    #[serde(default)]
    pub supports_reason_code: bool,
    #[serde(default)]
    pub supports_tags: bool,
    #[serde(default)]
    pub supports_facts: bool,
    #[serde(default)]
    pub supports_trade_levels: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SignalMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<SignalStrength>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub facts: BTreeMap<String, IndicatorFactValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trade_levels: Option<IndicatorTradeLevels>,
}

impl SignalMetadata {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strength.is_none()
            && self.reason_code.is_none()
            && self.tags.is_empty()
            && self.facts.is_empty()
            && self.trade_levels.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SignalMetadataFilter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_strengths: Vec<SignalStrength>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_reason_codes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_tags_all: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_tags_any: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_tags: Vec<String>,
}

impl SignalMetadataFilter {
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.allowed_strengths.is_empty()
            || !self.allowed_reason_codes.is_empty()
            || !self.required_tags_all.is_empty()
            || !self.required_tags_any.is_empty()
            || !self.blocked_tags.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IndicatorSignalMetadataFilters {
    #[serde(default)]
    pub entry: SignalMetadataFilter,
    #[serde(default)]
    pub exit: SignalMetadataFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorRole {
    PrimarySignal,
    Filter,
    Context,
    RiskHelper,
    ResearchOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorStabilityClass {
    StableOnClose,
    PreviewOnly,
    PivotDelayed,
    ZigzagRevisable,
    ParityOnlyUnsafe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorSignalPolicy {
    PreviewAllowed,
    ConfirmedRequired,
}

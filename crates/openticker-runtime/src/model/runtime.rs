use crate::{
    AccountConfig, AccountLedger, ConnectorRegistry, DataPlaneConfig, OperatorReadModels,
    RuntimeJournal, ServiceObservability,
};
pub(crate) use openticker_lane::LaneRuntime;
pub use openticker_lane::{LaneRecoveryState, LaneRuntimeState};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Runtime {
    pub(crate) state: RuntimeState,
    pub(crate) catalog: RuntimeCatalog,
    pub(crate) connectors: Arc<Mutex<ConnectorRegistry>>,
    pub(crate) journal: Arc<dyn RuntimeJournal>,
    pub(crate) read_models: Arc<OperatorReadModels>,
}

#[derive(Debug)]
pub(crate) struct RuntimeState {
    pub(crate) lanes: HashMap<String, LaneRuntime>,
    pub(crate) kill_switch_active: bool,
    pub(crate) observability: ServiceObservability,
}

#[derive(Debug)]
pub(crate) struct RuntimeCatalog {
    pub(crate) lanes_by_bot: HashMap<String, Vec<String>>,
    pub(crate) accounts: HashMap<String, AccountConfig>,
    pub(crate) account_ledgers: HashMap<String, Arc<Mutex<AccountLedger>>>,
    pub(crate) data_plane_config: DataPlaneConfig,
}

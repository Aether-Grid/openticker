#[allow(clippy::wildcard_imports)]
use crate::*;

mod accounting;
mod bootstrap;
mod journal;
mod lanes;
mod ledgers;
mod provider_events;
mod reconciliation;
mod summaries;

pub(crate) use bootstrap::bootstrap_journal_state;

#[derive(Clone, Copy)]
pub(crate) struct RuntimeRepoRead<'a> {
    pub(crate) catalog: &'a RuntimeCatalog,
    pub(crate) state: &'a RuntimeState,
    pub(crate) journal: &'a dyn RuntimeJournal,
    pub(crate) read_models: &'a OperatorReadModels,
}

pub(crate) struct RuntimeRepo<'a> {
    pub(crate) catalog: &'a RuntimeCatalog,
    pub(crate) state: &'a mut RuntimeState,
    pub(crate) journal: &'a dyn RuntimeJournal,
    pub(crate) read_models: &'a OperatorReadModels,
}

impl RuntimeRepo<'_> {
    pub(crate) fn as_read(&self) -> RuntimeRepoRead<'_> {
        RuntimeRepoRead {
            catalog: self.catalog,
            state: self.state,
            journal: self.journal,
            read_models: self.read_models,
        }
    }

    pub(crate) fn reborrow(&mut self) -> RuntimeRepo<'_> {
        RuntimeRepo {
            catalog: self.catalog,
            state: self.state,
            journal: self.journal,
            read_models: self.read_models,
        }
    }
}

impl Runtime {
    pub(crate) fn repo(&self) -> RuntimeRepoRead<'_> {
        RuntimeRepoRead {
            catalog: &self.catalog,
            state: &self.state,
            journal: self.journal.as_ref(),
            read_models: self.read_models.as_ref(),
        }
    }

    pub(crate) fn repo_mut(&mut self) -> RuntimeRepo<'_> {
        RuntimeRepo {
            catalog: &self.catalog,
            state: &mut self.state,
            journal: self.journal.as_ref(),
            read_models: self.read_models.as_ref(),
        }
    }

    pub(crate) fn instance(&self, lane_id: &str) -> Result<&LaneRuntime, ServiceError> {
        self.state
            .lanes
            .get(lane_id)
            .ok_or_else(|| ServiceError::InstanceNotFound(lane_id.to_owned()))
    }

    pub(crate) fn instance_mut(&mut self, lane_id: &str) -> Result<&mut LaneRuntime, ServiceError> {
        self.state
            .lanes
            .get_mut(lane_id)
            .ok_or_else(|| ServiceError::InstanceNotFound(lane_id.to_owned()))
    }

    pub(crate) fn lane_ids_for_bot_cloned(
        &self,
        bot_id: &str,
    ) -> Result<Vec<String>, ServiceError> {
        self.repo().lane_ids_for_bot_cloned(bot_id)
    }

    pub(crate) fn resolve_lane_id(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<String, ServiceError> {
        self.repo().resolve_lane_id(bot_id, symbol)
    }

    pub(crate) fn single_lane_id_for_bot(
        &self,
        bot_id: &str,
        action: &'static str,
    ) -> Result<String, ServiceError> {
        self.repo().single_lane_id_for_bot(bot_id, action)
    }

    pub(crate) fn transition_instance(
        &mut self,
        lane_id: &str,
        target_state: LaneRuntimeState,
        event_kind: &str,
    ) -> Result<InstanceSummary, ServiceError> {
        self.repo_mut()
            .transition_instance(lane_id, target_state, event_kind)
    }

    pub(crate) fn persist_boot_state(&self) -> Result<(), ServiceError> {
        self.repo().persist_boot_state()
    }

    pub(crate) fn append_instance_event(
        &self,
        lane_id: &str,
        kind: &str,
        state: LaneRuntimeState,
        detail: &str,
    ) -> Result<(), ServiceError> {
        self.repo()
            .append_instance_event(lane_id, kind, state, detail)
    }

    pub(crate) fn append_service_event(
        &self,
        kind: &str,
        payload: String,
    ) -> Result<(), ServiceError> {
        self.repo().append_service_event(kind, payload)
    }

    pub(crate) fn append_order_record(
        &self,
        lane_id: &str,
        client_order_id: &str,
        intent: TradeIntent,
        status: &str,
        price: f64,
        quantity: f64,
    ) -> Result<(), ServiceError> {
        self.repo()
            .append_order_record(lane_id, client_order_id, intent, status, price, quantity)
    }

    pub(crate) fn append_position_record(
        &self,
        lane_id: &str,
        has_position: bool,
        quantity: f64,
        entry_price: Option<f64>,
        realized_pnl_usd: f64,
        reason: &str,
    ) -> Result<(), ServiceError> {
        self.repo().append_position_record(
            lane_id,
            has_position,
            quantity,
            entry_price,
            realized_pnl_usd,
            reason,
        )
    }

    pub(crate) fn append_runtime_event(
        &self,
        scope: &str,
        entity_id: Option<&str>,
        kind: &str,
        payload: String,
    ) -> Result<(), ServiceError> {
        self.repo()
            .append_runtime_event(scope, entity_id, kind, payload)
    }
}

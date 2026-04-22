use super::{RuntimeRepo, RuntimeRepoRead};
use crate::{LaneRuntime, ServiceError};

impl RuntimeRepoRead<'_> {
    pub(crate) fn instance(&self, lane_id: &str) -> Result<&LaneRuntime, ServiceError> {
        self.state
            .lanes
            .get(lane_id)
            .ok_or_else(|| ServiceError::InstanceNotFound(lane_id.to_owned()))
    }

    pub(crate) fn lane_ids_for_bot(&self, bot_id: &str) -> Result<&Vec<String>, ServiceError> {
        self.catalog
            .lanes_by_bot
            .get(bot_id)
            .ok_or_else(|| ServiceError::InstanceNotFound(bot_id.to_owned()))
    }

    pub(crate) fn lane_ids_for_bot_cloned(
        &self,
        bot_id: &str,
    ) -> Result<Vec<String>, ServiceError> {
        Ok(self.lane_ids_for_bot(bot_id)?.clone())
    }

    pub(crate) fn lanes_for_bot(&self, bot_id: &str) -> Result<Vec<&LaneRuntime>, ServiceError> {
        self.lane_ids_for_bot(bot_id)?
            .iter()
            .map(|lane_id| self.instance(lane_id))
            .collect()
    }

    pub(crate) fn resolve_lane_id(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<String, ServiceError> {
        let normalized_symbol = symbol.trim();
        if normalized_symbol.is_empty() {
            return Err(ServiceError::SymbolNotConfigured {
                instance_id: bot_id.to_owned(),
                symbol: symbol.to_owned(),
            });
        }

        for lane_id in self.lane_ids_for_bot(bot_id)? {
            let lane = self.instance(lane_id)?;
            if lane.lane_symbol == normalized_symbol {
                return Ok(lane_id.clone());
            }
        }

        Err(ServiceError::SymbolNotConfigured {
            instance_id: bot_id.to_owned(),
            symbol: normalized_symbol.to_owned(),
        })
    }

    pub(crate) fn single_lane_id_for_bot(
        &self,
        bot_id: &str,
        action: &'static str,
    ) -> Result<String, ServiceError> {
        let lane_ids = self.lane_ids_for_bot(bot_id)?;
        if lane_ids.len() == 1 {
            Ok(lane_ids[0].clone())
        } else {
            Err(ServiceError::SymbolSelectionRequired {
                instance_id: bot_id.to_owned(),
                action,
            })
        }
    }
}

impl RuntimeRepo<'_> {
    pub(crate) fn instance_mut(&mut self, lane_id: &str) -> Result<&mut LaneRuntime, ServiceError> {
        self.state
            .lanes
            .get_mut(lane_id)
            .ok_or_else(|| ServiceError::InstanceNotFound(lane_id.to_owned()))
    }
}

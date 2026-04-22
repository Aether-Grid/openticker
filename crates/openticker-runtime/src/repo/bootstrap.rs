use crate::{InstanceConfig, RuntimeJournal, ServiceError};
use openticker_lane::lane_instance_id;
use std::collections::HashMap;

pub(crate) struct BootstrapJournalState {
    pub(crate) snapshot_states: HashMap<String, String>,
    pub(crate) recovered_realized_pnl_by_lane: HashMap<String, f64>,
}

pub(crate) fn bootstrap_journal_state(
    instances: &[InstanceConfig],
    journal: &dyn RuntimeJournal,
) -> Result<BootstrapJournalState, ServiceError> {
    Ok(BootstrapJournalState {
        snapshot_states: snapshot_states(journal)?,
        recovered_realized_pnl_by_lane: recovered_realized_pnl_by_lane(instances, journal)?,
    })
}

fn snapshot_states(journal: &dyn RuntimeJournal) -> Result<HashMap<String, String>, ServiceError> {
    Ok(journal
        .load_bot_snapshots()?
        .into_iter()
        .map(|snapshot| (snapshot.bot_id, snapshot.state))
        .collect::<HashMap<_, _>>())
}

fn recovered_realized_pnl_by_lane(
    instances: &[InstanceConfig],
    journal: &dyn RuntimeJournal,
) -> Result<HashMap<String, f64>, ServiceError> {
    let mut recovered_realized_pnl_by_lane = HashMap::new();

    for instance in instances {
        for symbol in &instance.symbols {
            let lane_id = lane_instance_id(instance, symbol).map_err(ServiceError::from)?;
            let latest_position = journal.latest_position_for_lane(&instance.id, symbol)?;
            let latest_position = match latest_position {
                Some(position) => Some(position),
                None if instance.symbols.len() == 1 => {
                    journal.latest_position_for_bot(&instance.id)?
                }
                None => None,
            };

            let Some(realized_pnl_usd) = latest_position.map(|position| position.realized_pnl_usd)
            else {
                continue;
            };

            if realized_pnl_usd.is_finite() {
                recovered_realized_pnl_by_lane.insert(lane_id, realized_pnl_usd);
            }
        }
    }

    Ok(recovered_realized_pnl_by_lane)
}

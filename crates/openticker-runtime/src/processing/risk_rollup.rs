use crate::Runtime;

impl Runtime {
    pub(super) fn refresh_daily_loss_rollover(&mut self, current_date: chrono::NaiveDate) {
        for instance in self.state.lanes.values_mut() {
            if instance.last_loss_reset_date != Some(current_date) {
                instance.daily_loss_pct_accumulated = 0.0;
                instance.last_loss_reset_date = Some(current_date);
            }
        }
    }
}

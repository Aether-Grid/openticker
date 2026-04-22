use super::RuntimeRepoRead;
use crate::{AccountLedger, LedgerOwnerPath, ServiceError, ledger_owner_path};
use std::sync::{Arc, Mutex};

impl RuntimeRepoRead<'_> {
    pub(crate) fn account_ledger_handle(
        &self,
        account_id: &str,
    ) -> Result<Arc<Mutex<AccountLedger>>, ServiceError> {
        self.catalog
            .account_ledgers
            .get(account_id)
            .cloned()
            .ok_or_else(|| {
                ServiceError::InvalidConfiguration(format!(
                    "missing capital ledger for account `{account_id}`"
                ))
            })
    }

    pub(crate) fn ledger_owner_path_for_instance(
        &self,
        instance_id: &str,
    ) -> Result<LedgerOwnerPath, ServiceError> {
        Ok(ledger_owner_path(self.instance(instance_id)?))
    }
}

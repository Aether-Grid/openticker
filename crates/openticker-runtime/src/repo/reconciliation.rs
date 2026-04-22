use super::RuntimeRepoRead;
use crate::{ConnectorAccountSnapshot, ConnectorOpenOrder, LedgerException, ServiceError};
use openticker_portfolio::{
    ClassifiedRemoteOpenOrders, LocalOpenOrderIdentity, open_orders_for_symbol,
    unmapped_managed_open_order_exceptions,
};
use std::collections::{HashMap, HashSet};

impl RuntimeRepoRead<'_> {
    pub(crate) fn classify_remote_open_orders(
        &self,
        account_id: &str,
        symbol: &str,
        orders: &[ConnectorOpenOrder],
    ) -> Result<ClassifiedRemoteOpenOrders, ServiceError> {
        let matches_by_client_order_id =
            self.local_open_order_matches_by_client_order_id(orders)?;
        let eligible_bot_ids = self.managed_bot_ids_for_account_symbol(account_id, symbol);

        Ok(openticker_portfolio::classify_remote_open_orders(
            account_id,
            symbol,
            orders,
            &matches_by_client_order_id,
            &eligible_bot_ids,
        ))
    }

    pub(crate) fn unmapped_managed_open_order_exceptions_for_snapshot(
        &self,
        account_id: &str,
        snapshot: &ConnectorAccountSnapshot,
        symbols: &[String],
    ) -> Result<Vec<LedgerException>, ServiceError> {
        let mut exceptions = Vec::new();

        for symbol in symbols {
            let classified = self.classify_remote_open_orders(
                account_id,
                symbol,
                &open_orders_for_symbol(snapshot, symbol),
            )?;
            exceptions.extend(unmapped_managed_open_order_exceptions(symbol, &classified));
        }

        Ok(exceptions)
    }

    fn local_open_order_matches_by_client_order_id(
        &self,
        orders: &[ConnectorOpenOrder],
    ) -> Result<HashMap<String, Vec<LocalOpenOrderIdentity>>, ServiceError> {
        let mut matches_by_client_order_id = HashMap::with_capacity(orders.len());

        for order in orders {
            if matches_by_client_order_id.contains_key(order.client_order_id.as_str()) {
                continue;
            }

            let matches = self
                .orders_by_client_order_id(order.client_order_id.as_str())?
                .into_iter()
                .map(|record| LocalOpenOrderIdentity {
                    bot_id: record.bot_id,
                    symbol: record.symbol,
                })
                .collect::<Vec<_>>();
            matches_by_client_order_id.insert(order.client_order_id.clone(), matches);
        }

        Ok(matches_by_client_order_id)
    }

    fn managed_bot_ids_for_account_symbol(
        &self,
        account_id: &str,
        symbol: &str,
    ) -> HashSet<String> {
        self.accounting_lanes_for_account(account_id)
            .into_iter()
            .filter(|lane| lane.symbol == symbol)
            .map(|lane| lane.bot_id)
            .collect()
    }
}

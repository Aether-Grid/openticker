use super::RuntimeRepoRead;
use crate::ServiceError;
use serde_json::{Map, Value, json};

pub(crate) struct ProviderOperationLog<'a> {
    repo: RuntimeRepoRead<'a>,
    lane_id: String,
    kind_prefix: String,
    account_id: String,
    connector_kind: String,
    operation: String,
}

impl RuntimeRepoRead<'_> {
    pub(crate) fn provider_operation(
        &self,
        lane_id: &str,
        kind_prefix: &str,
        account_id: &str,
        connector_kind: &str,
        operation: &str,
    ) -> ProviderOperationLog<'_> {
        ProviderOperationLog {
            repo: *self,
            lane_id: lane_id.to_owned(),
            kind_prefix: kind_prefix.to_owned(),
            account_id: account_id.to_owned(),
            connector_kind: connector_kind.to_owned(),
            operation: operation.to_owned(),
        }
    }
}

impl ProviderOperationLog<'_> {
    pub(crate) fn record_stage(
        &self,
        stage: &str,
        summary: impl Into<String>,
        extra: Value,
    ) -> Result<(), ServiceError> {
        let mut payload = Map::from_iter([
            ("account_id".to_owned(), json!(self.account_id)),
            ("connector_kind".to_owned(), json!(self.connector_kind)),
            ("operation".to_owned(), json!(self.operation)),
            ("stage".to_owned(), json!(stage)),
            ("summary".to_owned(), json!(summary.into())),
        ]);

        if let Value::Object(extra) = extra {
            payload.extend(extra);
        }

        self.repo.append_provider_event(
            &self.lane_id,
            &format!("{}.{}", self.kind_prefix, stage),
            Value::Object(payload),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_call<T, E>(
        &self,
        request_stage: &str,
        request_summary: impl Into<String>,
        request_extra: Value,
        call: impl FnOnce() -> Result<T, E>,
        success: impl FnOnce(&T) -> (&'static str, String, Value),
        failure: impl FnOnce(&E) -> (String, Value),
        map_error: impl FnOnce(E) -> ServiceError,
    ) -> Result<T, ServiceError> {
        self.record_stage(request_stage, request_summary, request_extra)?;

        match call() {
            Ok(result) => {
                let (stage, summary, extra) = success(&result);
                self.record_stage(stage, summary, extra)?;
                Ok(result)
            }
            Err(error) => {
                let (summary, extra) = failure(&error);
                let _ = self.record_stage("failed", summary, extra);
                Err(map_error(error))
            }
        }
    }
}

use toml::Table;

pub(crate) fn indicator_param_f64(params: &Table, key: &str) -> Option<f64> {
    params.get(key).and_then(|value| {
        value.as_float().or_else(|| {
            value
                .as_integer()
                .and_then(|integer| integer.to_string().parse::<f64>().ok())
        })
    })
}

#[allow(clippy::redundant_closure_for_method_calls)]
pub(crate) fn indicator_param_usize(params: &Table, key: &str) -> Option<usize> {
    params
        .get(key)
        .and_then(|value| value.as_integer())
        .and_then(|integer| usize::try_from(integer).ok())
}

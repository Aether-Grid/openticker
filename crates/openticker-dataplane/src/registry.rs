use crate::stream::{StreamKey, StreamSource, StreamSpec, compare_stream_key};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct StreamRegistry {
    specs: HashMap<StreamKey, StreamSpec>,
}

impl StreamRegistry {
    #[must_use]
    pub fn from_specs(specs: impl IntoIterator<Item = StreamSpec>) -> Self {
        let mut registry = Self::default();
        for spec in specs {
            registry.insert(spec);
        }
        registry
    }

    pub fn insert(&mut self, spec: StreamSpec) {
        match self.specs.get_mut(&spec.key) {
            Some(existing) => merge_stream_spec(existing, spec),
            None => {
                self.specs
                    .insert(spec.key.clone(), normalized_stream_spec(spec));
            }
        }
    }

    #[must_use]
    pub fn specs(&self) -> Vec<StreamSpec> {
        let mut specs = self.specs.values().cloned().collect::<Vec<_>>();
        specs.sort_by(|left, right| compare_stream_key(&left.key, &right.key));
        specs
    }
}

fn normalized_stream_spec(mut spec: StreamSpec) -> StreamSpec {
    spec.sources = normalized_sources(spec.sources);
    spec
}

fn merge_stream_spec(existing: &mut StreamSpec, next: StreamSpec) {
    existing.retention = existing.retention.max(next.retention);
    existing.polling_interval_ms = existing.polling_interval_ms.min(next.polling_interval_ms);
    existing.preview_enabled |= next.preview_enabled;
    existing.sources.extend(next.sources);
    existing.sources = normalized_sources(existing.sources.clone());
}

fn normalized_sources(sources: Vec<StreamSource>) -> Vec<StreamSource> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for source in sources {
        if seen.insert(source.clone()) {
            normalized.push(source);
        }
    }
    normalized.sort_by(compare_stream_source);
    normalized
}

fn compare_stream_source(left: &StreamSource, right: &StreamSource) -> Ordering {
    match (left, right) {
        (StreamSource::Watchlist, StreamSource::Watchlist) => Ordering::Equal,
        (StreamSource::Watchlist, StreamSource::Instance(_)) => Ordering::Less,
        (StreamSource::Instance(_), StreamSource::Watchlist) => Ordering::Greater,
        (StreamSource::Instance(left), StreamSource::Instance(right)) => left.cmp(right),
    }
}

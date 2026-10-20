//! Audits span lifetimes for thread-local, strictly nested profiling scopes.
//! Set `HELM_SCHEMA_SPAN_CHECK_FIVE_CHARTS` to audit the five heavy corpus recipes.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::ThreadId;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema_test_support::{generate, registry};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;
use tracing::span::{Attributes, Id};
use tracing::{Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt as _};
use tracing_subscriber::registry::LookupSpan;

struct OpenSpan {
    id: Id,
    metadata: &'static Metadata<'static>,
}

struct CreatedOn {
    thread: ThreadId,
}

#[derive(Default)]
struct Lifetimes {
    stacks: HashMap<ThreadId, Vec<OpenSpan>>,
    violations: BTreeMap<String, usize>,
    opened: usize,
}

#[derive(Clone, Default)]
struct LifetimeLayer {
    state: Arc<Mutex<Lifetimes>>,
}

fn site(metadata: &Metadata<'_>) -> String {
    format!(
        "{} at {}:{}",
        metadata.name(),
        metadata.file().unwrap_or("unknown"),
        metadata.line().unwrap_or(0)
    )
}

impl<S> Layer<S> for LifetimeLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let thread = std::thread::current().id();
        if let Some(span) = context.span(id) {
            span.extensions_mut().insert(CreatedOn { thread });
        }
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.opened += 1;
        state.stacks.entry(thread).or_default().push(OpenSpan {
            id: id.clone(),
            metadata: attributes.metadata(),
        });
    }

    fn on_close(&self, id: Id, context: Context<'_, S>) {
        let Some(span) = context.span(&id) else {
            return;
        };
        let extensions = span.extensions();
        let Some(created) = extensions.get::<CreatedOn>() else {
            return;
        };
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if created.thread != std::thread::current().id() {
            *state
                .violations
                .entry(format!("cross-thread close: {}", site(span.metadata())))
                .or_default() += 1;
        }
        let Some(stack) = state.stacks.get_mut(&created.thread) else {
            return;
        };
        let Some(position) = stack.iter().position(|open| open.id == id) else {
            return;
        };
        let violation = stack.last().filter(|top| top.id != id).map(|top| {
            format!(
                "non-LIFO close: {}; still open: {}",
                site(span.metadata()),
                site(top.metadata)
            )
        });
        // Remove the actual span so one bad close does not cascade into unrelated reports.
        stack.remove(position);
        if let Some(violation) = violation {
            *state.violations.entry(violation).or_default() += 1;
        }
    }
}

#[test]
fn corpus_spans_close_in_thread_local_stack_order() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let layer = LifetimeLayer::default();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(layer.clone().with_filter(
            tracing_subscriber::filter::filter_fn(|metadata| {
                metadata.target().starts_with("helm_schema")
            }),
        )),
    )?;
    let charts = if std::env::var_os("HELM_SCHEMA_SPAN_CHECK_FIVE_CHARTS").is_some() {
        vec![
            "openebs",
            "oncall",
            "kube-prometheus-stack",
            "milvus",
            "bitnami-redis",
        ]
    } else {
        vec!["bitnami-redis"]
    };
    let specs = registry::registry()
        .into_iter()
        .filter(|spec| {
            charts
                .iter()
                .any(|chart| spec.id.key() == format!("chart/{chart}"))
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: specs.len(), want: charts.len());
    let out = ScratchDir::new("span-lifetimes")?;
    let entries = generate::produce_entries(&specs, out.path(), 1, false)?;
    for entry in entries {
        let fixture = entry
            .fixture
            .ok_or_else(|| eyre::eyre!("missing fixture for {}", entry.file))?;
        let actual =
            std::fs::read(out.path().join(&entry.file)).wrap_err("read generated artifact")?;
        let expected =
            std::fs::read(test_util::workspace_root().join(fixture)).wrap_err("read fixture")?;
        sim_assert_eq!(have: actual, want: expected, "{}", entry.file);
    }
    let state = layer.state.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(state.opened > 0, "the chart audit must observe spans");
    let unclosed = state.stacks.values().map(Vec::len).sum::<usize>();
    eprintln!(
        "spans_opened={} unclosed={unclosed} violations={:?}",
        state.opened, state.violations
    );
    sim_assert_eq!(have: &state.violations, want: &BTreeMap::<String, usize>::new());
    sim_assert_eq!(have: unclosed, want: 0);
    Ok(())
}

#[test]
fn checker_detects_non_lifo_and_cross_thread_closes() {
    let layer = LifetimeLayer::default();
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer.clone()));
    tracing::dispatcher::with_default(&dispatch, || {
        // Unentered siblings do not retain each other through parent references.
        let first = tracing::info_span!("first");
        let second = tracing::info_span!("second");
        drop(first);
        drop(second);

        let moved = tracing::info_span!("moved");
        std::thread::scope(|scope| {
            scope.spawn(move || drop(moved));
        });
    });
    let state = layer.state.lock().unwrap_or_else(PoisonError::into_inner);
    sim_assert_eq!(have: state.violations.values().sum::<usize>(), want: 2);
    assert!(
        state
            .violations
            .keys()
            .any(|key| key.starts_with("non-LIFO close:"))
    );
    assert!(
        state
            .violations
            .keys()
            .any(|key| key.starts_with("cross-thread close:"))
    );
    sim_assert_eq!(have: state.stacks.values().map(Vec::len).sum::<usize>(), want: 0);
}

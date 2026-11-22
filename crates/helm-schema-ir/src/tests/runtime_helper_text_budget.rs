use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Executor, HelperBranch, HelperTarget, State, Value};
use crate::analysis_db::IrAnalysisDb;
use crate::value::{Exit, Flow, TextBuffer};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::ValuesPath;
use test_util::prelude::sim_assert_eq;
use tracing_subscriber::prelude::*;

struct OutputCrossings(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for OutputCrossings {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = attributes.metadata();
        if metadata.target() == "helm_schema_ir::counters" && metadata.name() == "output_bound" {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(in crate::value) fn count_crossings<T>(run: impl FnOnce() -> T) -> (T, usize) {
    let crossings = Arc::new(AtomicUsize::new(0));
    let subscriber = tracing_subscriber::registry().with(OutputCrossings(Arc::clone(&crossings)));
    let value = tracing::subscriber::with_default(subscriber, run);
    (value, crossings.load(Ordering::Relaxed))
}

pub(in crate::value) fn run_capture(
    source: &str,
    argument: &Value,
    state: State,
) -> eyre::Result<TextBuffer> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let program = db.parsed_source(source, None, "capture");
    let executions = Executor { db: &db }.execute_helper_branch(
        "include",
        HelperBranch {
            state,
            target: HelperTarget::Program(Rc::clone(&program.root)),
        },
        argument,
        &[],
    );
    sim_assert_eq!(have: executions.len(), want: 1);
    let Exit { state, flow } = executions.into_iter().next().ok_or_eyre("missing exit")?;
    assert!(matches!(flow, Flow::Normal(_)));
    Ok(state.output)
}

#[test]
fn bounded_helper_capture_keeps_terms_through_symbolic_forks_and_joins() -> eyre::Result<()> {
    let output = run_capture(
        "before{{ if . }}same{{ else }}same{{ end }}after",
        &Value::input(ValuesPath::parse("choice")),
        State::new(Value::nil()),
    )?;
    assert!(output.counter.is_none());
    sim_assert_eq!(have: output.exact_text(), want: Some("beforesameafter".to_owned()));

    let source = r#"x{{ "y" }}"#.repeat(63) + "z";
    let output = run_capture(&source, &Value::nil(), State::new(Value::nil()))?;
    assert!(output.counter.is_none());
    sim_assert_eq!(have: output.exact_text(), want: Some("xy".repeat(63) + "z"));
    Ok(())
}

#[test]
fn oversized_and_repeated_capture_widen_on_the_129th_term() -> eyre::Result<()> {
    let flat = r#"x{{ "y" }}"#.repeat(64);
    let repeated = r#"{{ range 32 }}x{{ "y" }}x{{ "y" }}{{ end }}"#.to_owned();
    for source in [flat, repeated] {
        let (output, crossings) =
            count_crossings(|| run_capture(&source, &Value::nil(), State::new(Value::nil())));
        let output = output?;
        assert!(output.counter.is_some());
        sim_assert_eq!(have: output.exact_text(), want: Some("xy".repeat(64)));
        sim_assert_eq!(have: crossings, want: 0);
        let (output, crossings) = count_crossings(|| {
            run_capture(&(source + "z"), &Value::nil(), State::new(Value::nil()))
        });
        let output = output?;
        assert!(output.counter.is_some());
        sim_assert_eq!(have: output.exact_text(), want: None);
        sim_assert_eq!(have: crossings, want: 1);
    }
    Ok(())
}

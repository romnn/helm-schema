use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::BoundValueContext;
use crate::eval_effect::MemberHostConversion;
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::ScalarValueDispatch;
use helm_schema_core::Predicate;

/// Abstract interpreter environment for Helm expression evaluation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EvalEnv {
    pub(crate) dot: Option<AbstractValue>,
    pub(crate) root_fields: HashMap<String, AbstractValue>,
    pub(crate) root_truthy_predicates: HashMap<String, Predicate>,
    pub(crate) root_value_dispatches: HashMap<String, ScalarValueDispatch>,
    /// The root-field semantic maps describe the current dot rather than
    /// only the global root. Bound helpers set this for their call argument
    /// frame; nested `with`/`range` frames clear it.
    pub(crate) root_field_semantics_on_current_dot: bool,
    pub(crate) locals: HashMap<String, AbstractValue>,
    /// Exact scalar values carried by locals, including branch-dependent
    /// reassignments. This is separate from `locals`: an abstract fragment
    /// identifies where a value came from, while a scalar dispatch identifies
    /// the runtime value that equality and truthiness consume.
    pub(crate) local_scalar_dispatches: HashMap<String, ScalarValueDispatch>,
    /// Locals bound by `:=`/`=` rather than by `range`. Go stores a pipeline
    /// result through `reflect.ValueOf(value.Interface())`, which turns a nil
    /// interface into an INVALID value, and field access on an invalid
    /// receiver yields zero instead of aborting — so navigating one of these
    /// is nil-safe for its own hop, while a range member variable (set
    /// straight from `MapIndex`) keeps the abort.
    pub(crate) pipeline_bound_locals: std::collections::HashSet<String>,
    pub(crate) local_default_paths: HashMap<String, BTreeSet<String>>,
    pub(crate) local_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    /// Structural conditions under which a local is truthy, as the fragment
    /// interpreter reduced them. A boolean flag carries no values-path
    /// identity, so short-circuit operand truthiness has no other way to
    /// decode it.
    pub(crate) local_truthy_reductions: HashMap<String, Predicate>,
    pub(crate) member_host_conversions: BTreeSet<MemberHostConversion>,
    pub(crate) active_predicates: Vec<Predicate>,
    pub(crate) bound_values: BoundValueContext,
    pub(crate) allow_field_root_lookup: bool,
    pub(crate) skip_helper_call_args: bool,
}

impl EvalEnv {
    pub(crate) fn from_helper_context(
        bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
    ) -> Self {
        Self {
            dot: current_dot.cloned(),
            root_fields: bindings.cloned().unwrap_or_default(),
            allow_field_root_lookup: true,
            ..Self::default()
        }
    }

    pub(crate) fn from_fragment_context(
        locals: &HashMap<String, AbstractValue>,
        current_dot: Option<&AbstractValue>,
    ) -> Self {
        Self {
            dot: current_dot.cloned(),
            root_fields: locals.clone(),
            locals: locals.clone(),
            allow_field_root_lookup: false,
            ..Self::default()
        }
    }

    pub(crate) fn without_helper_call_args(mut self) -> Self {
        self.skip_helper_call_args = true;
        self
    }

    pub(crate) fn apply_local_set_mutations(
        &mut self,
        mutations: &BTreeMap<String, BTreeMap<String, AbstractValue>>,
    ) -> bool {
        let mut applied = false;
        for (name, entries) in mutations {
            let Some(value) = self.locals.remove(name) else {
                continue;
            };
            self.locals
                .insert(name.clone(), value.with_overlay_entries(entries.clone()));
            applied = true;
        }
        applied
    }
}

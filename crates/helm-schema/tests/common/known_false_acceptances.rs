//! Accepted documents that Helm or Kubernetes reject, that the battery
//! knows.
//!
//! A baseline that rejects its own chart defaults rejects every probe exactly
//! as it rejects the defaults, so a probe it rejects carries no evidence and
//! is judged on the candidate alone. A candidate acceptance of a document
//! Helm aborts on or the pinned Kubernetes v1.29 bundle rejects is then a
//! false acceptance the analyzer already had: one the baseline hid behind its
//! blanket rejection. A baseline whose violations differ from its defaults'
//! is evidence, and a false acceptance behind it is listed only with an
//! adjudication of why that baseline rejected the cell. Each false
//! acceptance is listed with its suspected family. The battery fails on a
//! cell missing from the roster and on an entry that no longer fails, which
//! must be removed. Accepted renders the pinned evidence cannot decide are
//! listed in `known_undecided_acceptances.rs`.

use std::str::FromStr;

use serde::Serialize;

/// How the render of an accepted document fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Rejection {
    /// `helm template` aborts.
    HelmAborts,
    /// Helm renders, and the pinned Kubernetes bundle rejects the render.
    KubernetesRejects,
}

/// How the baseline schema rejected a false acceptance's cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum Baseline {
    /// With exactly the violations of the chart's own defaults.
    RejectsItsDefaults,
    /// With violations that differ from those of the chart's own defaults.
    RejectsUnlikeItsDefaults,
}

/// A defect family of the schema-bug-hunt-v1 campaign, as numbered in
/// `plan/schema-bug-hunt-v1.md`.
///
/// The false-acceptance roster below and the frozen-witness catalog
/// (`family_witnesses.rs`) both attribute their rows to these families.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Family {
    /// F0: an unversioned external CRD catalog overrides chart-local structural
    /// facts.
    F0,
    /// F1: `global` is absent from a closed root, so the chart cannot be a
    /// dependency.
    F1,
    /// F2: `hasKey` is lowered as "present and non-null".
    F2,
    /// F3: `else if .Values.X` inside a chain with a total `else` makes `X`
    /// mandatory.
    F3,
    /// F4: a label or annotation map read from a `toYaml` operand.
    F4,
    /// F5: `range` over a value Helm cannot iterate.
    F5,
    /// F6: bitnami validation and `fail` helpers (`common.resources.preset`,
    /// `common.errors.insecureImages`, the image pull secret helpers).
    F6,
    /// F7: an `and` guard containing any undecidable conjunct loses the whole
    /// guard.
    F7,
    /// F8: a capability-guarded region produces no facts at all.
    F8,
    /// F9: a null deletion reaches a typed sink; nil is modelled, its type
    /// is not.
    F9,
    /// F10: builtin coverage gaps decide whether a sibling is modelled.
    F10,
    /// F11: `.Values` captured into a variable escapes analysis.
    F11,
    /// F12: a rendered-sink type constraint is pushed back onto the input,
    /// ignoring the formatter.
    F12,
    /// F13: YAML well-formedness, `toYaml` of a string where a list or map
    /// is expected, or a render that is no manifest.
    F13,
    /// F14: a combinator erases the next pipe stage's type obligation.
    F14,
    /// F15: recovered enum literals are discarded by an open alternative.
    F15,
    /// F16: a ranged collection is bound to the sink it constructs.
    F16,
    /// F17: a `kindIs` dispatch loses one of its arms.
    F17,
    /// F18: `required <value> <message>` with reversed arguments emits no
    /// requirement.
    F18,
    /// F19: member requirements are lost through a statically enumerable range.
    F19,
    /// F20: a Kubernetes `int-or-string` union is collapsed to the default's
    /// scalar type.
    F20,
    /// F21: a constraint escapes the guard arm its siblings sit inside.
    F21,
    /// F22: nil-dereference aborts inside a `range` body are not recorded.
    F22,
    /// F23: subchart-scoped nil-deref arms are emitted null-only, so they are
    /// dead.
    F23,
    /// F24: vacuous reject arms: the intended constraint is silently absent.
    F24,
    /// F25: the analyzer computes the correct guard, then emits an unguarded
    /// duplicate.
    F25,
    /// F26: "reaches a rendered-string position" is read as "is a string".
    F26,
    /// F27: `coalesce` is modelled order-free.
    F27,
    /// F28: wrapping the subject in a function loses the path binding.
    F28,
    /// F29: a helper's guard is taken from only one of its call sites.
    F29,
    /// F30: a numeric-looking string rendered as a plain scalar re-types.
    F30,
    /// F31: the provider type is not pushed back through a pass-through
    /// helper.
    F31,
    /// F32: Helm built-in context roots leak into a values path.
    F32,
    /// F33: `append` over a nil list is not modelled.
    F33,
    /// F34: an `or` guard drops an ordering-comparison disjunct.
    F34,
    /// F35: an `else if` chain emitting the same key loses the `else if`
    /// condition.
    F35,
    /// F36: a recovered type is widened with `null` and emitted without the
    /// matching `required`, so a null deletion reaches the abort.
    F36,
    /// F37: an `IntOrString` union makes `null` fail `oneOf`, so the value is
    /// forced mandatory.
    F37,
    /// F38: a single-line `with` resolves the sink to the enclosing item.
    F38,
    /// F39: a provider `oneOf` is kept verbatim while its branches are
    /// rewritten.
    F39,
    /// F40: a plain-scalar preimage is computed per hole, not over the composed
    /// scalar.
    F40,
    /// F41: a value rendered into a mapping-key slot is under-constrained.
    F41,
    /// F42: an unmodelled call anywhere in a guard conjunct discards the
    /// branch.
    F42,
    /// F43: `default <literal> .Values.X` pins `X` to the literal's type.
    F43,
    /// F44: `required` over a `dig` rooted at a `.Values` sub-path drops its
    /// terminal.
    F44,
    /// F45: `not (len X)` lowers to "absent or null" rather than "empty".
    F45,
    /// F46: an undecidable `.Capabilities` conjunct is *dropped* from an abort
    /// guard.
    F46,
    /// F47: a `define` between an accumulator and its `fail` deletes the whole
    /// arm.
    F47,
    /// F48: a leaf contract two wildcard levels deep is dropped.
    F48,
    /// F49: function-catalogue omissions decide whether a contract exists.
    F49,
    /// F50: an `or`-selected alias conjoins its candidates instead of case-
    /// splitting.
    F50,
    /// F51: an `or`-selected alias path is misclassified when parenthesized.
    F51,
    /// F52: Go's `and` returns its operand, not a boolean.
    F52,
    /// F53: a self-truthiness gate drops the operand's *kind* contract, not
    /// just presence.
    F53,
    /// F54: a `range` target is constrained against every scalar type except
    /// string.
    F54,
    /// F55: `mustMergeOverwrite` with a non-literal operand drops every sub-
    /// path fact.
    F55,
    /// F56: a guard reading a branch-reassigned variable deletes the whole
    /// region.
    F56,
    /// F57: a helper invoked with a positional `list` context contributes no
    /// facts.
    F57,
    /// F58: a nested `range` drops the inner range's item constraints.
    F58,
    /// F59: `eq`/`ne` comparability is hoisted out of its guard.
    F59,
    /// F60: a provider constraint lands on the values field that *names* a key.
    F60,
    /// F61: a constraint is attributed to a path that is never read.
    F61,
    /// F62: subchart-namespace nil-dereference obligations are not emitted at
    /// all.
    F62,
    /// F63: a `range`-derived item shape omits `type: "object"`.
    F63,
    /// F64: an accumulate-then-`fail` chain through a formatter is unmodelled.
    F64,
    /// F65: a pipeline operand in a comparison drops the entire guarded region.
    F65,
    /// F66: `not (keys X)` is modelled as "X null or absent", missing `{}`.
    F66,
    /// F67: a local `set $v …` erases an obligation that `$v` established.
    F67,
    /// F68: a builtin's Go `string` parameter aborts on nil, and the
    /// requirement is unmodelled.
    F68,
    /// F69: `kindIs` dispatch arms are intersected, so the admitted domain
    /// collapses to `null`.
    F69,
    /// F70: an `or` guard is satisfied by one operand while the *other* reaches
    /// a typed parameter as `null`.
    F70,
    /// F71: a `semverCompare` gate over a helper-derived version is omitted
    /// entirely.
    F71,
    /// F72: a `toYaml` operand is modelled as a nil-navigation abort, so the
    /// key becomes mandatory.
    F72,
    /// F73: the root object is closed with no template-derived reason.
    F73,
    /// F74: the emitted schema exceeds Helm's file-size limit, so the chart
    /// cannot be installed.
    F74,
    /// F75: a numeric `gt` / `lt` guard in front of a `fail` produces no
    /// constraint.
    F75,
    /// F76: a constraint from an unconditional statement is conjoined with a
    /// later branch condition, inverted.
    F76,
    /// F77: an unconditional string-operand fact is lost when a sibling
    /// condition split collapses.
    F77,
    /// F78: an object-type fact from a subchart member access is lost the same
    /// way.
    F78,
    /// F79: the round-74 oracle files legitimate provider constraints as false
    /// rejections.
    F79,
    /// F80: declared-shape typing on config-text interpolations collides with
    /// the flip law.
    F80,
    /// D4: a `.Subcharts` reference leaks a subchart's facts into the chart's
    /// scope.
    D4,
    /// D5: a block scalar whose body starts inside a control region.
    D5,
    /// D3: colliding template basenames contaminate one chart with another's
    /// templates. Not one of the 83 campaign families.
    D3,
    /// B6: an escaped container loses its guard. An in-flight track, not one of
    /// the 83 campaign families.
    B6,
    /// L1: `| default` literal type hints inside a tpl-evaluated `with` are
    /// emitted unguarded. Not one of the 83 campaign families.
    L1,
    /// L2: merge attribution types `.securityContext.capabilities` as a whole
    /// `SecurityContext`. Not one of the 83 campaign families.
    L2,
    /// No family filed yet: a hard-coded API version the pinned server no
    /// longer serves, an empty `{}` item rendered as a document, a nil
    /// dereference of a deleted table, a non-empty conjunct of a `kindIs`
    /// dispatch arm, a value re-parsed from a helper's `toJson` or `toYaml`
    /// output, a renderable default the schema rejects for a reason no
    /// family names.
    Unfiled,
}

impl FromStr for Family {
    type Err = String;

    /// Parses a family label as the plan spells it, such as `F69` or `D5`.
    fn from_str(label: &str) -> Result<Self, Self::Err> {
        let family = match label {
            "F0" => Self::F0,
            "F1" => Self::F1,
            "F2" => Self::F2,
            "F3" => Self::F3,
            "F4" => Self::F4,
            "F5" => Self::F5,
            "F6" => Self::F6,
            "F7" => Self::F7,
            "F8" => Self::F8,
            "F9" => Self::F9,
            "F10" => Self::F10,
            "F11" => Self::F11,
            "F12" => Self::F12,
            "F13" => Self::F13,
            "F14" => Self::F14,
            "F15" => Self::F15,
            "F16" => Self::F16,
            "F17" => Self::F17,
            "F18" => Self::F18,
            "F19" => Self::F19,
            "F20" => Self::F20,
            "F21" => Self::F21,
            "F22" => Self::F22,
            "F23" => Self::F23,
            "F24" => Self::F24,
            "F25" => Self::F25,
            "F26" => Self::F26,
            "F27" => Self::F27,
            "F28" => Self::F28,
            "F29" => Self::F29,
            "F30" => Self::F30,
            "F31" => Self::F31,
            "F32" => Self::F32,
            "F33" => Self::F33,
            "F34" => Self::F34,
            "F35" => Self::F35,
            "F36" => Self::F36,
            "F37" => Self::F37,
            "F38" => Self::F38,
            "F39" => Self::F39,
            "F40" => Self::F40,
            "F41" => Self::F41,
            "F42" => Self::F42,
            "F43" => Self::F43,
            "F44" => Self::F44,
            "F45" => Self::F45,
            "F46" => Self::F46,
            "F47" => Self::F47,
            "F48" => Self::F48,
            "F49" => Self::F49,
            "F50" => Self::F50,
            "F51" => Self::F51,
            "F52" => Self::F52,
            "F53" => Self::F53,
            "F54" => Self::F54,
            "F55" => Self::F55,
            "F56" => Self::F56,
            "F57" => Self::F57,
            "F58" => Self::F58,
            "F59" => Self::F59,
            "F60" => Self::F60,
            "F61" => Self::F61,
            "F62" => Self::F62,
            "F63" => Self::F63,
            "F64" => Self::F64,
            "F65" => Self::F65,
            "F66" => Self::F66,
            "F67" => Self::F67,
            "F68" => Self::F68,
            "F69" => Self::F69,
            "F70" => Self::F70,
            "F71" => Self::F71,
            "F72" => Self::F72,
            "F73" => Self::F73,
            "F74" => Self::F74,
            "F75" => Self::F75,
            "F76" => Self::F76,
            "F77" => Self::F77,
            "F78" => Self::F78,
            "F79" => Self::F79,
            "F80" => Self::F80,
            "D4" => Self::D4,
            "D5" => Self::D5,
            "D3" => Self::D3,
            "B6" => Self::B6,
            "L1" => Self::L1,
            "L2" => Self::L2,
            "Unfiled" => Self::Unfiled,
            _ => return Err(format!("unknown family label {label:?}")),
        };
        Ok(family)
    }
}

/// A probe: `path` set to a value of the probe value class `value`.
pub(crate) struct Probe {
    pub(crate) path: &'static str,
    pub(crate) value: &'static str,
}

/// The known false acceptances of one chart that fail alike behind the same
/// kind of baseline rejection and share a suspected family.
pub(crate) struct KnownFalseAcceptances {
    pub(crate) chart: &'static str,
    pub(crate) rejection: Rejection,
    pub(crate) baseline: Baseline,
    pub(crate) family: Family,
    pub(crate) probes: &'static [Probe],
}

/// The baseline commit every roster row was adjudicated against. A row is
/// a cell that baseline rejected and the candidate accepts, so the battery
/// observes it only as a flip against this baseline; against any other
/// baseline the rows cannot be observed at all.
pub(crate) const ROSTER_BASELINE: &str = "f7be7ba52ba6401f527cf47ce62767343a0b1485";

pub(crate) const KNOWN_FALSE_ACCEPTANCES: &[KnownFalseAcceptances] = &[
    KnownFalseAcceptances {
        chart: "dify",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "dify",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F9,
        probes: &[Probe {
            path: "pluginDaemon.persistence.size",
            value: "null deletion [depth 3]",
        }],
    },
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F31,
        probes: &[
            Probe {
                path: "graylog.affinity",
                value: "coercible string",
            },
            Probe {
                path: "graylog.affinity",
                value: "empty object item",
            },
            Probe {
                path: "graylog.affinity",
                value: "non-coercible string",
            },
            Probe {
                path: "graylog.affinity",
                value: "number",
            },
            Probe {
                path: "graylog.affinity",
                value: "true",
            },
            Probe {
                path: "graylog.affinity",
                value: "unknown object member",
            },
            Probe {
                path: "graylog.terminationGracePeriodSeconds",
                value: "non-coercible string",
            },
        ],
    },
    // `quote` of the deleted value renders `version:` empty (mongodb-community.yaml:9), so
    // the pinned MongoDBCommunity CRD rejects `/spec/version: null is not of type "string"`;
    // the emitted `version` is `{}`.
    KnownFalseAcceptances {
        chart: "graylog",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F9,
        probes: &[Probe {
            path: "mongodb.community.version",
            value: "null deletion [depth 3]",
        }],
    },
    KnownFalseAcceptances {
        chart: "oncall",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F13,
        probes: &[
            Probe {
                path: "celery.extraContainers",
                value: "coercible string",
            },
            Probe {
                path: "celery.extraContainers",
                value: "non-coercible string",
            },
            Probe {
                path: "engine.extraContainers",
                value: "coercible string",
            },
            Probe {
                path: "engine.extraContainers",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "oncall",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    // The baseline's image arms were intersected to null-only (F25/F69), so it rejects
    // the default image maps as well. `okteto.fullImage` prints the string arm bare
    // (`image: {{ include "okteto.image.backend" . }}`, webhook-deployment.yaml:86 and
    // every other image sink), so "3" renders the integer 3 and the pinned bundle
    // rejects `containers/0/image: 3 is not of types "null", "string"`; the candidate's
    // string arm admits it.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsUnlikeItsDefaults,
        family: Family::F30,
        probes: &[
            Probe {
                path: "backend.image",
                value: "coercible string",
            },
            Probe {
                path: "buildkit.image",
                value: "coercible string",
            },
            Probe {
                path: "daemonset.image",
                value: "coercible string",
            },
            Probe {
                path: "defaultBackend.image",
                value: "coercible string",
            },
            Probe {
                path: "frontend.image",
                value: "coercible string",
            },
            Probe {
                path: "installer.runner",
                value: "coercible string",
            },
            Probe {
                path: "redis.image",
                value: "coercible string",
            },
            Probe {
                path: "registry.image",
                value: "coercible string",
            },
        ],
    },
    // The baseline admitted only null here (F25/F69) and rejects the default image maps.
    // `okteto.fullImage` fails in its `else` arm on nil (_image.tpl:28, Helm: "Invalid
    // type for image value. Must be string or map."); both emitted `kindIs` arms are
    // `["null", ...]` with no `required`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsUnlikeItsDefaults,
        family: Family::F36,
        probes: &[
            Probe {
                path: "backend.image",
                value: "null deletion",
            },
            Probe {
                path: "buildkit.image",
                value: "null deletion",
            },
            Probe {
                path: "daemonset.image",
                value: "null deletion",
            },
            Probe {
                path: "defaultBackend.image",
                value: "null deletion",
            },
            Probe {
                path: "frontend.image",
                value: "null deletion",
            },
            Probe {
                path: "installer.runner",
                value: "null deletion",
            },
            Probe {
                path: "redis.image",
                value: "null deletion",
            },
            Probe {
                path: "registry.image",
                value: "null deletion",
            },
        ],
    },
    // The baseline admitted only null here (F25/F69) and rejects the default image maps.
    // The `kindIs "string"` arm also requires `ne $image ""` (_image.tpl:13), so ""
    // reaches the `fail` (Helm: "Invalid type for image value. Must be string or map.");
    // the emitted string arm drops that conjunct. No family covers a dispatch arm's
    // non-empty conjunct.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsUnlikeItsDefaults,
        family: Family::Unfiled,
        probes: &[
            Probe {
                path: "backend.image",
                value: "empty string",
            },
            Probe {
                path: "buildkit.image",
                value: "empty string",
            },
            Probe {
                path: "daemonset.image",
                value: "empty string",
            },
            Probe {
                path: "defaultBackend.image",
                value: "empty string",
            },
            Probe {
                path: "frontend.image",
                value: "empty string",
            },
            Probe {
                path: "installer.runner",
                value: "empty string",
            },
            Probe {
                path: "redis.image",
                value: "empty string",
            },
            Probe {
                path: "registry.image",
                value: "empty string",
            },
        ],
    },
    // `toYaml` of a list under the `annotations:` map (registry-deployment.yaml:47-48)
    // is no YAML (Helm: "did not find expected key"); the emitted union admits arrays.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F13,
        probes: &[Probe {
            path: "registry.annotations",
            value: "empty object item",
        }],
    },
    // `{unknown: true}` renders the annotation value `true` (registry-deployment.yaml:48),
    // and the pinned bundle rejects `annotations/unknown: true is not of types "null",
    // "string"`; the emitted `{"type": "object"}` arm has no string member type.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F4,
        probes: &[Probe {
            path: "registry.annotations",
            value: "unknown object member",
        }],
    },
    // `range .Values.redis.args` (redis-deployment.yaml:67) aborts with "range can't
    // iterate over 0"; the emitted type admits `integer`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F5,
        probes: &[Probe {
            path: "redis.args",
            value: "integer",
        }],
    },
    // `okteto.resourceManager.deletePeriodDays` fails when `lt (int $days) 1`
    // (_helpers.tpl:911-913, Helm: "resourceManager.deletePeriodDays must be an integer
    // greater than zero"), and `int` maps every one of these to 0; the emitted
    // `deletePeriodDays` is `{}`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F75,
        probes: &[
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty array",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty object",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty object item",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "empty string",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "false",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "integer",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "non-coercible string",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "null deletion",
            },
            Probe {
                path: "resourceManager.deletePeriodDays",
                value: "unknown object member",
            },
        ],
    },
    // `okteto.clusterrolebinding` passes the value through to `roleRef.name` bare
    // (_helpers.tpl:179, okteto-cluster-admin-rbac.yaml:16), and the pinned bundle
    // rejects `/roleRef/name: <value> is not of type "string"` (a deleted value renders
    // `name:` empty, i.e. null); the emitted value carries only a description.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F31,
        probes: &[
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "empty array",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "false",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "integer",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "null deletion",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "number",
            },
            Probe {
                path: "serviceAccounts.clusterRoleBinding",
                value: "true",
            },
        ],
    },
    // "3" reaches the bare `roleRef.name` (okteto-cluster-admin-rbac.yaml:16) as the
    // integer 3: `/roleRef/name: 3 is not of type "string"`.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "serviceAccounts.clusterRoleBinding",
            value: "coercible string",
        }],
    },
    // `[{}]` renders `name: [map[]]` at okteto-cluster-admin-rbac.yaml:16, which is no
    // YAML (Helm: "did not find expected ',' or ']'").
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F13,
        probes: &[Probe {
            path: "serviceAccounts.clusterRoleBinding",
            value: "empty object item",
        }],
    },
    // `okteto.serviceaccountsextrarolebindings` re-serializes the value with `toJson`
    // (_helpers.tpl:227), and the template ranges over its `fromJson` parse as a map of
    // lists (okteto-cluster-admin-rbac.yaml:25-29); Helm aborts with "range can't
    // iterate over json: cannot unmarshal ... into Go value of type
    // map[string]interface {}", or "range can't iterate over true" for a member. The
    // emitted value is `{}`. No family covers a value re-parsed from a helper's
    // serialized output.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "coercible string",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "empty array",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "empty object item",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "empty string",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "false",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "integer",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "non-coercible string",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "number",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "true",
            },
            Probe {
                path: "serviceAccounts.extraRoleBindings",
                value: "unknown object member",
            },
        ],
    },
    // `okteto.registry.storage.provider` prints `toYaml` of the value, and
    // `okteto.registry.storage.cloudProvider` re-parses it with `fromYaml` and reads
    // `$providerObj.aws.enabled` (_registry.tpl:25,32): Helm aborts with "nil pointer
    // evaluating interface {}.enabled". The emitted value is `{}`. No family covers a
    // value re-parsed from a helper's serialized output.
    KnownFalseAcceptances {
        chart: "okteto",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "registry.storage.provider",
            value: "null deletion [depth 3]",
        }],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F13,
        probes: &[
            Probe {
                path: "extraDeploy",
                value: "unknown object member",
            },
            Probe {
                path: "extraEnvVars",
                value: "coercible string",
            },
            Probe {
                path: "extraEnvVars",
                value: "non-coercible string",
            },
            Probe {
                path: "extraVolumeMounts",
                value: "coercible string",
            },
            Probe {
                path: "extraVolumeMounts",
                value: "non-coercible string",
            },
            Probe {
                path: "extraVolumes",
                value: "coercible string",
            },
            Probe {
                path: "extraVolumes",
                value: "non-coercible string",
            },
            Probe {
                path: "image.tag",
                value: "empty string",
            },
            Probe {
                path: "networkPolicy.extraIngress",
                value: "coercible string",
            },
            Probe {
                path: "networkPolicy.extraIngress",
                value: "non-coercible string",
            },
            Probe {
                path: "service.extraPorts",
                value: "coercible string",
            },
            Probe {
                path: "service.extraPorts",
                value: "non-coercible string",
            },
            Probe {
                path: "sidecars",
                value: "coercible string",
            },
            Probe {
                path: "sidecars",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F4,
        probes: &[
            Probe {
                path: "commonAnnotations",
                value: "coercible string",
            },
            Probe {
                path: "commonAnnotations",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F5,
        probes: &[
            Probe {
                path: "extraDeploy",
                value: "integer",
            },
            Probe {
                path: "global.imagePullSecrets",
                value: "integer",
            },
            Probe {
                path: "image.pullSecrets",
                value: "integer",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F6,
        probes: &[
            Probe {
                path: "certificates.image.registry",
                value: "null deletion [depth 3]",
            },
            Probe {
                path: "certificates.image.repository",
                value: "null deletion [depth 3]",
            },
            Probe {
                path: "global.imageRegistry",
                value: "coercible string",
            },
            Probe {
                path: "global.imageRegistry",
                value: "non-coercible string",
            },
            Probe {
                path: "global.imageRegistry",
                value: "unknown object member",
            },
            Probe {
                path: "image.registry",
                value: "coercible string",
            },
            Probe {
                path: "image.registry",
                value: "empty array",
            },
            Probe {
                path: "image.registry",
                value: "empty object",
            },
            Probe {
                path: "image.registry",
                value: "false",
            },
            Probe {
                path: "image.registry",
                value: "integer",
            },
            Probe {
                path: "image.registry",
                value: "non-coercible string",
            },
            Probe {
                path: "image.registry",
                value: "null deletion",
            },
            Probe {
                path: "image.registry",
                value: "unknown object member",
            },
            Probe {
                path: "image.repository",
                value: "coercible string",
            },
            Probe {
                path: "image.repository",
                value: "non-coercible string",
            },
            Probe {
                path: "resourcesPreset",
                value: "coercible string",
            },
            Probe {
                path: "resourcesPreset",
                value: "empty string",
            },
            Probe {
                path: "resourcesPreset",
                value: "non-coercible string",
            },
            Probe {
                path: "resourcesPreset",
                value: "null deletion",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[
            Probe {
                path: "extraEnvVarsCM",
                value: "coercible string",
            },
            Probe {
                path: "extraEnvVarsSecret",
                value: "coercible string",
            },
            Probe {
                path: "fullnameOverride",
                value: "coercible string",
            },
            Probe {
                path: "global.defaultStorageClass",
                value: "coercible string",
            },
            Probe {
                path: "persistence.existingClaim",
                value: "coercible string",
            },
            Probe {
                path: "persistence.storageClass",
                value: "coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F31,
        probes: &[
            Probe {
                path: "affinity",
                value: "coercible string",
            },
            Probe {
                path: "affinity",
                value: "non-coercible string",
            },
            Probe {
                path: "args",
                value: "coercible string",
            },
            Probe {
                path: "args",
                value: "non-coercible string",
            },
            Probe {
                path: "command",
                value: "coercible string",
            },
            Probe {
                path: "command",
                value: "non-coercible string",
            },
            Probe {
                path: "containerPorts.http",
                value: "integer",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "integer",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.capabilities",
                value: "unknown object member",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty object",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.runAsGroup",
                value: "unknown object member",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty object",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.runAsUser",
                value: "unknown object member",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "coercible string",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "empty array",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "empty object item",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "empty string",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "false",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "integer",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "non-coercible string",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "number",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "true",
            },
            Probe {
                path: "containerSecurityContext.seLinuxOptions",
                value: "unknown object member",
            },
            Probe {
                path: "customLivenessProbe",
                value: "coercible string",
            },
            Probe {
                path: "customLivenessProbe",
                value: "non-coercible string",
            },
            Probe {
                path: "customPostInitScripts",
                value: "coercible string",
            },
            Probe {
                path: "customPostInitScripts",
                value: "non-coercible string",
            },
            Probe {
                path: "customReadinessProbe",
                value: "coercible string",
            },
            Probe {
                path: "customReadinessProbe",
                value: "non-coercible string",
            },
            Probe {
                path: "customStartupProbe",
                value: "coercible string",
            },
            Probe {
                path: "customStartupProbe",
                value: "non-coercible string",
            },
            Probe {
                path: "global.imagePullSecrets",
                value: "unknown object member",
            },
            Probe {
                path: "hostAliases",
                value: "coercible string",
            },
            Probe {
                path: "hostAliases",
                value: "non-coercible string",
            },
            Probe {
                path: "image.pullSecrets",
                value: "unknown object member",
            },
            Probe {
                path: "initContainers",
                value: "coercible string",
            },
            Probe {
                path: "initContainers",
                value: "non-coercible string",
            },
            Probe {
                path: "lifecycleHooks",
                value: "coercible string",
            },
            Probe {
                path: "lifecycleHooks",
                value: "non-coercible string",
            },
            Probe {
                path: "nodeSelector",
                value: "coercible string",
            },
            Probe {
                path: "nodeSelector",
                value: "non-coercible string",
            },
            Probe {
                path: "persistence.dataSource",
                value: "coercible string",
            },
            Probe {
                path: "persistence.dataSource",
                value: "non-coercible string",
            },
            Probe {
                path: "persistence.selector",
                value: "coercible string",
            },
            Probe {
                path: "persistence.selector",
                value: "non-coercible string",
            },
            Probe {
                path: "podAnnotations",
                value: "coercible string",
            },
            Probe {
                path: "podAnnotations",
                value: "non-coercible string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "coercible string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty array",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty object",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty object item",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "empty string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "false",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "non-coercible string",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "number",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "true",
            },
            Probe {
                path: "podSecurityContext.fsGroup",
                value: "unknown object member",
            },
            Probe {
                path: "service.sessionAffinityConfig",
                value: "coercible string",
            },
            Probe {
                path: "service.sessionAffinityConfig",
                value: "non-coercible string",
            },
            Probe {
                path: "tolerations",
                value: "coercible string",
            },
            Probe {
                path: "tolerations",
                value: "non-coercible string",
            },
            Probe {
                path: "topologySpreadConstraints",
                value: "coercible string",
            },
            Probe {
                path: "topologySpreadConstraints",
                value: "non-coercible string",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F9,
        probes: &[
            Probe {
                path: "persistence.size",
                value: "null deletion",
            },
            Probe {
                path: "service.ports.http",
                value: "null deletion [depth 3]",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "redmine",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "extraDeploy",
            value: "empty object item",
        }],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F13,
        probes: &[
            Probe {
                path: "dockerRegistries",
                value: "empty object item",
            },
            Probe {
                path: "halyard.image.tag",
                value: "null deletion [depth 3]",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "kubeConfig.onlySpinnakerManaged",
            value: "null deletion",
        }],
    },
    KnownFalseAcceptances {
        chart: "spinnaker",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[
            Probe {
                path: "ingress.enabled",
                value: "coercible string",
            },
            Probe {
                path: "ingress.enabled",
                value: "empty object item",
            },
            Probe {
                path: "ingress.enabled",
                value: "non-coercible string",
            },
            Probe {
                path: "ingress.enabled",
                value: "number",
            },
            Probe {
                path: "ingress.enabled",
                value: "true",
            },
            Probe {
                path: "ingress.enabled",
                value: "unknown object member",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "coercible string",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "empty object item",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "non-coercible string",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "number",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "true",
            },
            Probe {
                path: "ingressGate.enabled",
                value: "unknown object member",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "coercible string",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "empty object item",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "non-coercible string",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "number",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "true",
            },
            Probe {
                path: "rbac.pspEnabled",
                value: "unknown object member",
            },
            Probe {
                path: "minio.ingress.enabled",
                value: "true",
            },
        ],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F5,
        probes: &[Probe {
            path: "extraObjects",
            value: "integer",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F30,
        probes: &[Probe {
            path: "fullnameOverride",
            value: "coercible string",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::F9,
        probes: &[Probe {
            path: "persistence.size",
            value: "null deletion",
        }],
    },
    KnownFalseAcceptances {
        chart: "weblate",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: &[Probe {
            path: "extraObjects",
            value: "empty object item",
        }],
    },
];

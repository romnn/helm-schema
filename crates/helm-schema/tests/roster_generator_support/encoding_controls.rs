//! Frozen byte-distinct controls survive removal of their repaired landing roster rows.

use crate::known_false_acceptances::{ContextSelector, ContextTransport};
use crate::known_inherited_failures::{
    FailureClass, FailureInput, FailureSelector, KnownInheritedFailure,
};
use std::borrow::Cow;

pub(crate) const ENCODING_CONTROLS: &[KnownInheritedFailure] = &[
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0e17ad3087e6ccb8fabde25fade9373058646399a5cc600251408d1c66660ecd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintCoalescedTwice,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1f81105527cccb13e752dba9ad9b2ee2040e6ce3020a3edd6b77c5f0bd2f820b",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "0e17ad3087e6ccb8fabde25fade9373058646399a5cc600251408d1c66660ecd",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
    KnownInheritedFailure {
        chart: "oncall",
        selector: FailureSelector::Context(ContextSelector {
            document: test_util::helm_values::AcceptanceDocument::LintRaw,
            overlay: r#"{"postgresql":{"enabled":true}}"#,
            transport: ContextTransport::ValuesFile,
            kubernetes_version: "1.29.0",
        }),
        failure: FailureClass::Adjudication {
            reason: "Rust screening disagrees with the corresponding real lint rule",
        },
        input: FailureInput {
            values_file_sha256: Cow::Borrowed(
                "1f81105527cccb13e752dba9ad9b2ee2040e6ce3020a3edd6b77c5f0bd2f820b",
            ),
            controls_sha256: Cow::Borrowed(
                "215a8e7cbf3875fe1df5af1a59bf79533548a4a20efbc61455f13ad58ffcd70e",
            ),
        },
        lint_diagnostics_sha256: Some(Cow::Borrowed(
            "105533f02cde4f737fca6eb9eee708e8bed0f7e9380b6db62606eb4344a0938f",
        )),
        owner: "W4 regexp \\u (B)",
        observed_on: "landing10a/d7ab4078",
    },
];

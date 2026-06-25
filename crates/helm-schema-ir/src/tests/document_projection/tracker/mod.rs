use helm_schema_ast::DefineIndex;
use test_util::prelude::sim_assert_eq;

use crate::{ValueKind, YamlPath};

use super::attribution::{build_attribution_index, is_output_root_kind};
use super::{DocumentTracker, OutputSlot, OutputSlotKind};

fn parse_template(source: &str) -> tree_sitter::Tree {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("go-template grammar should load");
    parser.parse(source, None).expect("template should parse")
}

fn output_nodes_containing<'tree>(
    node: tree_sitter::Node<'tree>,
    source: &str,
    needle: &str,
    out: &mut Vec<tree_sitter::Node<'tree>>,
) {
    if is_output_root_kind(node.kind())
        && node
            .utf8_text(source.as_bytes())
            .is_ok_and(|text| text.contains(needle))
    {
        out.push(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        output_nodes_containing(child, source, needle, out);
    }
}

fn output_nodes_with_exact_text<'tree>(
    node: tree_sitter::Node<'tree>,
    source: &str,
    needle: &str,
    out: &mut Vec<tree_sitter::Node<'tree>>,
) {
    if is_output_root_kind(node.kind())
        && node
            .utf8_text(source.as_bytes())
            .is_ok_and(|text| text.trim() == needle)
    {
        out.push(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        output_nodes_with_exact_text(child, source, needle, out);
    }
}

fn nodes_with_text<'tree>(
    node: tree_sitter::Node<'tree>,
    source: &str,
    kind: &str,
    needle: &str,
    out: &mut Vec<tree_sitter::Node<'tree>>,
) {
    if node.kind() == kind
        && node
            .utf8_text(source.as_bytes())
            .is_ok_and(|text| text.contains(needle))
    {
        out.push(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        nodes_with_text(child, source, kind, needle, out);
    }
}

#[test]
fn output_slot_suppresses_fragment_output_for_mapping_keys() {
    let slot = OutputSlot {
        kind: ValueKind::Scalar,
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        resource: None,
        slot: OutputSlotKind::MappingKey,
    };

    assert!(slot.suppresses_fragment_output());
}

#[test]
fn output_slot_marks_partial_scalar_slots() {
    let slot = OutputSlot {
        kind: ValueKind::Scalar,
        path: YamlPath(vec!["spec".to_string(), "value".to_string()]),
        resource: None,
        slot: OutputSlotKind::PartialScalar,
    };

    sim_assert_eq!(have: slot.direct_value_kind(), want: ValueKind::PartialScalar);
    sim_assert_eq!(have: slot.path.0, want: vec!["spec", "value"]);
}

#[test]
fn attribution_uses_mapping_key_for_flow_sequence_scalar() {
    let source = r#"livenessProbe:
  exec:
    command: ['/bin/bash', '-c', 'echo "ruok" | timeout {{ .Values.timeout }} nc -w {{ .Values.timeout }} localhost {{ .Values.port }} | grep imok']
"#;
    let tree = parse_template(source);
    let attribution = build_attribution_index(source, tree.root_node());
    let mut nodes = Vec::new();
    output_nodes_containing(tree.root_node(), source, ".Values.timeout", &mut nodes);
    assert!(!nodes.is_empty());

    for node in nodes {
        let context = attribution
            .output_slot_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.path.0,
            want: vec!["livenessProbe", "exec", "command"],
            "node kind {}",
            node.kind()
        );
    }
}

#[test]
fn tracker_keeps_outer_prefix_for_fragment_inside_with_body() {
    let source =
        include_str!("../../../../../../testdata/charts/cert-manager/templates/deployment.yaml");
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "toYaml", &mut actions);
    let action = actions
        .into_iter()
        .find(|node| {
            node.utf8_text(source.as_bytes()).is_ok_and(|text| {
                text.contains("nindent 8")
                    && source[..node.start_byte()].contains("with .Values.volumes")
            })
        })
        .expect("fragment action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "template", "spec", "volumes"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0,
    );
}

#[test]
fn tracker_attributes_cert_manager_extra_env_to_container_env() {
    let source =
        include_str!("../../../../../../testdata/charts/cert-manager/templates/deployment.yaml");
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let range_start = source
        .find("with .Values.extraEnv")
        .expect("extraEnv with block");
    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "toYaml", &mut actions);
    let action = actions
        .into_iter()
        .filter(|node| node.start_byte() > range_start)
        .min_by_key(tree_sitter::Node::start_byte)
        .expect("extraEnv render action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "template", "spec", "containers[*]", "env"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_fragment_below_container_security_context_key() {
    let source = r#"{{- if .Values.web.enabled -}}
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include "temporal.componentname" (list $ "web") }}
spec:
  replicas: {{ .Values.web.replicaCount }}
  template:
    metadata:
      labels:
        {{- include "temporal.resourceLabels" (list $ "web" "pod") | nindent 8 }}
    spec:
      {{- with .Values.web.additionalInitContainers }}
      initContainers:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{ include "temporal.serviceAccount" $ }}
      {{- if .Values.web.additionalVolumes }}
      volumes:
      {{- toYaml .Values.web.additionalVolumes | nindent 8 }}
      {{- end }}
      containers:
        - name: {{ .Chart.Name }}-web
          image: "{{ .Values.web.image.repository }}:{{ .Values.web.image.tag }}"
          imagePullPolicy: {{ .Values.web.image.pullPolicy }}
          env:
            - name: TEMPORAL_ADDRESS
              value: "{{ include "temporal.fullname" $ }}-frontend.{{ .Release.Namespace }}.svc:{{ .Values.server.frontend.service.port }}"
          {{- if .Values.web.additionalEnv }}
          {{- toYaml .Values.web.additionalEnv | nindent 12 }}
          {{- end }}
          {{- if .Values.web.additionalEnvSecretName }}
          envFrom:
            - secretRef:
                name: {{ .Values.web.additionalEnvSecretName }}
          {{- end }}
          livenessProbe:
            initialDelaySeconds: 10
            tcpSocket:
              port: http
          ports:
            - name: http
              containerPort: 8080
              protocol: TCP
          resources:
            {{- toYaml .Values.web.resources | nindent 12 }}
          {{- with .Values.web.containerSecurityContext }}
          securityContext:
            {{- toYaml . | nindent 12 }}
          {{- end }}
          {{- with .Values.web.additionalVolumeMounts }}
          volumeMounts:
            {{- toYaml . | nindent 12 }}
          {{- end }}
      {{- with .Values.web.securityContext }}
      securityContext:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with $.Values.imagePullSecrets }}
      imagePullSecrets:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.web.nodeSelector }}
      nodeSelector:
        {{- toYaml . | nindent 8 }}
      {{- end }}
{{- end }}
"#;
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let security_context_start = source
        .find("with .Values.web.containerSecurityContext")
        .expect("container security context with block");
    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "toYaml", &mut actions);
    let action = actions
        .into_iter()
        .filter(|node| node.start_byte() > security_context_start)
        .min_by_key(tree_sitter::Node::start_byte)
        .expect("security context render");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec![
            "spec",
            "template",
            "spec",
            "containers[*]",
            "securityContext",
        ],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_cert_manager_inline_host_aliases_fragment_to_host_aliases() {
    let source =
        include_str!("../../../../../../testdata/charts/cert-manager/templates/deployment.yaml");
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let host_aliases_start = source
        .find("with .Values.hostAliases")
        .expect("host aliases");
    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "toYaml", &mut actions);
    let action = actions
        .into_iter()
        .filter(|node| node.start_byte() > host_aliases_start)
        .min_by_key(tree_sitter::Node::start_byte)
        .expect("host aliases render action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "template", "spec", "hostAliases"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_cert_manager_inline_ip_families_fragment_to_field() {
    let source =
        include_str!("../../../../../../testdata/charts/cert-manager/templates/service.yaml");
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "serviceIPFamilies", &mut actions);
    let action = actions
        .into_iter()
        .find(|node| {
            node.utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains("nindent 2"))
        })
        .expect("serviceIPFamilies render action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "ipFamilies"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_signoz_service_common_labels_to_metadata_labels() {
    let source = include_str!(
        "../../../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "commonLabels", &mut actions);
    let action = actions
        .into_iter()
        .find(|node| {
            node.utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains("common.tplvalues.render"))
        })
        .expect("commonLabels render action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["metadata", "labels"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_signoz_service_extra_ports_to_service_ports() {
    let source = include_str!(
        "../../../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "service.extraPorts", &mut actions);
    let action = actions
        .into_iter()
        .find(|node| {
            node.utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains("common.tplvalues.render"))
        })
        .expect("extraPorts render action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "ports"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_cert_manager_liveness_probe_scalar_to_probe_field() {
    let source =
        include_str!("../../../../../../testdata/charts/cert-manager/templates/deployment.yaml");
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, ".failureThreshold", &mut actions);
    let action = actions
        .into_iter()
        .find(|node| source[..node.start_byte()].contains("with .Values.livenessProbe"))
        .expect("liveness probe failureThreshold action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec![
            "spec",
            "template",
            "spec",
            "containers[*]",
            "livenessProbe",
            "failureThreshold",
        ],
        "node_kind={} node_text={:?} current={:?}",
        action.kind(),
        action.utf8_text(source.as_bytes()).unwrap_or(""),
        tracker.control_site_for_node(action).path.0
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar
    );
}

#[test]
fn tracker_attributes_cert_manager_proxy_value_to_env_value() {
    let source =
        include_str!("../../../../../../testdata/charts/cert-manager/templates/deployment.yaml");
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let proxy_start = source.find("HTTP_PROXY").expect("HTTP proxy env entry");
    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, ".", &mut actions);
    let action = actions
        .into_iter()
        .filter(|node| matches!(node.kind(), "template_action" | "dot"))
        .filter(|node| node.start_byte() > proxy_start)
        .min_by_key(tree_sitter::Node::start_byte)
        .expect("HTTP proxy value action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec![
            "spec",
            "template",
            "spec",
            "containers[*]",
            "env[*]",
            "value",
        ],
        "node_kind={} node_text={:?} current={:?}",
        action.kind(),
        action.utf8_text(source.as_bytes()).unwrap_or(""),
        tracker.control_site_for_node(action).path.0
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar
    );
}

#[test]
fn attribution_marks_mapping_value_action_as_entire_scalar() {
    let source = r#"env:
  - name: HTTP_PROXY
    value: {{ .Values.http_proxy }}
"#;
    let tree = parse_template(source);
    let attribution = build_attribution_index(source, tree.root_node());
    let mut nodes = Vec::new();
    output_nodes_containing(tree.root_node(), source, ".Values.http_proxy", &mut nodes);
    assert!(!nodes.is_empty());

    for node in nodes {
        let context = attribution
            .output_slot_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.path.0,
            want: vec!["env[*]", "value"],
            "node kind {}",
            node.kind()
        );
        sim_assert_eq!(
            have: context.slot,
            want: OutputSlotKind::WholeScalar,
            "node kind {} should be the entire scalar value",
            node.kind()
        );
    }
}

#[test]
fn attribution_marks_inline_sequence_mapping_value_action_as_entire_scalar() {
    let source = r#"ports:
  - port: {{ .Values.port }}
"#;
    let tree = parse_template(source);
    let attribution = build_attribution_index(source, tree.root_node());
    let mut nodes = Vec::new();
    output_nodes_containing(tree.root_node(), source, ".Values.port", &mut nodes);
    assert!(!nodes.is_empty());

    for node in nodes {
        let context = attribution
            .output_slot_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.path.0,
            want: vec!["ports[*]", "port"],
            "node kind {}",
            node.kind()
        );
        sim_assert_eq!(
            have: context.slot,
            want: OutputSlotKind::WholeScalar,
            "node kind {} should be the entire scalar value",
            node.kind()
        );
    }
}

#[test]
fn tracker_preserves_entire_scalar_for_inline_sequence_mapping_action() {
    let source = r#"ports:
  - port: {{ .Values.port }}
"#;
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, ".Values.port", &mut actions);
    let action = actions.into_iter().next().expect("output action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec!["ports[*]", "port"]
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar
    );
}

#[test]
fn tracker_preserves_entire_scalar_for_inline_sequence_mapping_action_in_control_body() {
    let source = r#"{{- if .Values.metrics.enabled }}
ports:
  - port: {{ .Values.metrics.containerPorts.http }}
{{- end }}
"#;
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.metrics.containerPorts.http",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("output action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec!["ports[*]", "port"]
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar
    );
}

#[test]
fn tracker_attributes_required_call_to_mapping_scalar_value() {
    let source = r#"env:
  - name: SMTP_FROM
    valueFrom:
      secretKeyRef:
        name: {{ required "secret name is missing" $.Values.signoz.smtpVars.existingSecret.name }}
"#;
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.signoz.smtpVars.existingSecret.name",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("required output action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec!["env[*]", "valueFrom", "secretKeyRef", "name",]
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar
    );
}

#[test]
fn tracker_attributes_signoz_smtp_required_name_to_secret_ref_name() {
    let source = include_str!(
        "../../../../../../testdata/charts/signoz-signoz/templates/signoz/statefulset.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.signoz.smtpVars.existingSecret.name",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("required output action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec![
            "spec",
            "template",
            "spec",
            "containers[*]",
            "env[*]",
            "valueFrom",
            "secretKeyRef",
            "name",
        ],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar
    );
}

#[test]
fn tracker_attributes_networkpolicy_extra_ingress_to_ingress_rules() {
    let source = include_str!(
        "../../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.networkPolicy.extraIngress",
        &mut actions,
    );
    let action = actions
        .into_iter()
        .find(|node| {
            node.utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains("common.tplvalues.render"))
        })
        .expect("extra ingress render action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "ingress"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_servicemonitor_metric_relabelings_to_endpoint_field() {
    let source =
        include_str!("../../../../../../testdata/charts/surveyor/templates/serviceMonitor.yaml");
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        "toYaml . | nindent 4",
        &mut actions,
    );
    let marker = source
        .find("metricRelabelings:")
        .expect("metricRelabelings key");
    let action = actions
        .into_iter()
        .filter(|node| node.start_byte() > marker)
        .min_by_key(tree_sitter::Node::start_byte)
        .expect("metricRelabelings render action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "endpoints[*]", "metricRelabelings"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_networkpolicy_standard_labels_to_metadata_labels() {
    let source = include_str!(
        "../../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        "common.labels.standard",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("standard labels action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["metadata", "labels"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_networkpolicy_match_labels_to_selector_matchlabels() {
    let source = include_str!(
        "../../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        "common.labels.matchLabels",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("match labels action");
    let path = tracker.output_slot_for_action(action).path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "podSelector", "matchLabels"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_networkpolicy_range_labels_to_matchlabels_map() {
    let source = include_str!(
        "../../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let range_start = source
        .find(r#"range $key, $value := .Values.networkPolicy.ingressNSMatchLabels"#)
        .expect("ingress namespace labels range");
    let mut actions = Vec::new();
    output_nodes_with_exact_text(tree.root_node(), source, "$value | quote", &mut actions);
    let action = actions
        .into_iter()
        .filter(|node| node.start_byte() > range_start)
        .min_by_key(tree_sitter::Node::start_byte)
        .expect("range value action");
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).path.0,
        want: vec![
            "spec",
            "ingress[*]",
            "from[*]",
            "namespaceSelector",
            "matchLabels",
        ],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_networkpolicy_metrics_range_labels_to_matchlabels_map() {
    let source = include_str!(
        "../../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let range_start = source
        .find(r#"range $key, $value := .Values.networkPolicy.metrics.ingressNSMatchLabels"#)
        .expect("metrics namespace labels range");
    let mut actions = Vec::new();
    output_nodes_with_exact_text(tree.root_node(), source, "$value | quote", &mut actions);
    let action = actions
        .into_iter()
        .filter(|node| node.start_byte() > range_start)
        .min_by_key(tree_sitter::Node::start_byte)
        .expect("metrics range value action");
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).path.0,
        want: vec![
            "spec",
            "ingress[*]",
            "from[*]",
            "namespaceSelector",
            "matchLabels",
        ],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_networkpolicy_range_mapping_entry_path() {
    let source = include_str!(
        "../../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut ranges = Vec::new();
    nodes_with_text(
        tree.root_node(),
        source,
        "range_action",
        ".Values.networkPolicy.ingressNSMatchLabels",
        &mut ranges,
    );
    let range = ranges.into_iter().next().expect("range action");
    sim_assert_eq!(
        have: tracker.control_site_for_node(range).range_mapping_entry_path
            .map(|path| path.0),
        want: Some(vec![
            "spec".to_string(),
            "ingress[*]".to_string(),
            "from[*]".to_string(),
            "namespaceSelector".to_string(),
            "matchLabels".to_string(),
        ]),
        "current={:?}",
        tracker.control_site_for_node(range).path.0
    );
}

#[test]
fn tracker_keeps_metadata_name_path_after_top_level_helper_include() {
    let source = r#"{{- include "synth.defaultValues" . }}
apiVersion: v1
kind: ServiceAccount
metadata:
  name: {{ .Values.serviceAccount.name | quote }}
"#;
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.serviceAccount.name",
        &mut actions,
    );
    let action = actions
        .into_iter()
        .next()
        .expect("serviceAccount.name output");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec!["metadata", "name"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar
    );
}

#[test]
fn tracker_attributes_signoz_storage_class_scalar_include_to_pvc_spec_container() {
    let source = include_str!(
        "../../../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        "common.storage.class",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("storage class include");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec!["spec", "volumeClaimTemplates[*]", "spec"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_signoz_storage_class_fragment_include_to_pvc_spec_container() {
    let source = include_str!(
        "../../../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        "common.storage.class",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("storage class include");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec!["spec", "volumeClaimTemplates[*]", "spec"],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_signoz_extra_volume_mounts_to_volume_mounts() {
    let source = include_str!(
        "../../../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(tree.root_node(), source, "extraVolumeMounts", &mut actions);
    let action = actions
        .into_iter()
        .find(|node| {
            node.utf8_text(source.as_bytes())
                .is_ok_and(|text| text.contains("nindent 12"))
        })
        .expect("extraVolumeMounts include");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec![
            "spec",
            "template",
            "spec",
            "containers[*]",
            "volumeMounts",
        ],
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_preserves_entire_scalar_for_bitnami_metrics_port_after_nested_blocks() {
    let source = include_str!(
        "../../../../../../testdata/charts/bitnami-redis/templates/networkpolicy.yaml"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.metrics.containerPorts.http",
        &mut actions,
    );
    let action = actions
        .into_iter()
        .next()
        .expect("metrics port output action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: vec!["spec", "ingress[*]", "ports[*]", "port"]
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::WholeScalar,
        "current={:?}",
        tracker.control_site_for_node(action).path.0
    );
}

#[test]
fn tracker_attributes_common_affinity_match_labels_include_to_nested_selector() {
    let source = include_str!(
        "../../../../../../testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates/_affinities.tpl"
    );
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let soft_helper = source
        .find(r#"define "common.affinities.pods.soft""#)
        .expect("soft affinity helper");
    let hard_helper = source
        .find(r#"define "common.affinities.pods.hard""#)
        .expect("hard affinity helper");
    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        "common.labels.matchLabels",
        &mut actions,
    );

    let soft_action = actions
        .iter()
        .copied()
        .find(|node| node.start_byte() > soft_helper && node.start_byte() < hard_helper)
        .expect("soft matchLabels include");
    sim_assert_eq!(
        have: tracker.output_slot_for_action(soft_action).path.0,
        want: vec![
            "preferredDuringSchedulingIgnoredDuringExecution[*]",
            "podAffinityTerm",
            "labelSelector",
            "matchLabels",
        ],
        "current={:?}",
        tracker.control_site_for_node(soft_action).path.0
    );

    let hard_action = actions
        .into_iter()
        .find(|node| node.start_byte() > hard_helper)
        .expect("hard matchLabels include");
    sim_assert_eq!(
        have: tracker.output_slot_for_action(hard_action).path.0,
        want: vec![
            "requiredDuringSchedulingIgnoredDuringExecution[*]",
            "labelSelector",
            "matchLabels",
        ],
        "current={:?}",
        tracker.control_site_for_node(hard_action).path.0
    );
}

#[test]
fn tracker_keeps_script_block_scalar_outputs_pathless() {
    let source = r#"args:
  - -ec
  - |
    chown -R {{ .Values.podSecurityContext.fsGroup }} /data
    {{- if .Values.dataLogDir }}
    mkdir -p {{ .Values.dataLogDir }}
    {{- end }}
"#;
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.podSecurityContext.fsGroup",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("script output action");
    sim_assert_eq!(
        have: tracker
            .output_slot_for_action(action)
            .path
            .0,
        want: Vec::<String>::new()
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::BlockScalarSuppressed
    );
}

#[test]
fn tracker_keeps_structural_fragment_inside_block_scalar_pathless() {
    let source = r#"data:
  config.yaml: |-
    persistence:
      {{- toYaml .Values.persistence.sql | nindent 6 }}
"#;
    let tree = parse_template(source);
    let defines = DefineIndex::new();
    let mut tracker = DocumentTracker::new(source, &defines);
    tracker.reset_for_tree(&tree);

    let mut actions = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.persistence.sql",
        &mut actions,
    );
    let action = actions.into_iter().next().expect("block scalar output");
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).path.0,
        want: Vec::<String>::new()
    );
    sim_assert_eq!(
        have: tracker.output_slot_for_action(action).slot,
        want: OutputSlotKind::BlockScalarSuppressed
    );
}

#[test]
fn attribution_marks_with_bound_dot_action_as_entire_scalar() {
    let source = r#"env:
  {{- with .Values.http_proxy }}
  - name: HTTP_PROXY
    value: {{ . }}
  {{- end }}
"#;
    let tree = parse_template(source);
    let attribution = build_attribution_index(source, tree.root_node());
    let mut nodes = Vec::new();
    output_nodes_with_exact_text(tree.root_node(), source, ".", &mut nodes);
    assert!(!nodes.is_empty());

    for node in nodes {
        let context = attribution
            .output_slot_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.path.0,
            want: vec!["env[*]", "value"],
            "node kind {}",
            node.kind()
        );
        sim_assert_eq!(
            have: context.slot,
            want: OutputSlotKind::WholeScalar,
            "node kind {} should be the entire scalar value",
            node.kind()
        );
    }
}

#[test]
fn attribution_marks_embedded_sequence_value_action_as_partial_scalar() {
    let source = r#"args:
  - --v={{ .Values.global.logLevel }}
"#;
    let tree = parse_template(source);
    let attribution = build_attribution_index(source, tree.root_node());
    let mut nodes = Vec::new();
    output_nodes_containing(
        tree.root_node(),
        source,
        ".Values.global.logLevel",
        &mut nodes,
    );
    assert!(!nodes.is_empty());

    for node in nodes {
        let context = attribution
            .output_slot_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.path.0,
            want: vec!["args[*]"],
            "node kind {}",
            node.kind()
        );
        sim_assert_eq!(
            have: context.slot,
            want: OutputSlotKind::PartialScalar,
            "node kind {} should be embedded in the scalar value",
            node.kind()
        );
    }
}

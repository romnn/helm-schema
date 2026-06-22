use helm_schema_ast::DefineIndex;
use test_util::prelude::sim_assert_eq;

use crate::ValueKind;

use super::DocumentTracker;
use super::attribution::{build_attribution_index, is_output_root_kind};

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
            .output_context_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.output_path.0,
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(8))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "template", "spec", "volumes"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 8).0,
        tracker.context_for_node(action).output_path.0,
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(10))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "template", "spec", "containers[*]", "env"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 10).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let rendered_context = tracker
        .attribution
        .virtual_indent_context_for_node(action, 12);
    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(12))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec![
            "spec",
            "template",
            "spec",
            "containers[*]",
            "securityContext",
        ],
        "current={:?} mapping={:?} context={:?} rendered={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 12).0,
        tracker.context_for_node(action),
        rendered_context
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(8))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "template", "spec", "hostAliases"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 8).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(2))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "ipFamilies"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 2).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(4))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["metadata", "labels"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 4).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(4))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "ports"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 4).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
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
        "node_kind={} node_text={:?} current={:?} mapping={:?} context={:?}",
        action.kind(),
        action.utf8_text(source.as_bytes()).unwrap_or(""),
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 12).0,
        tracker.context_for_node(action)
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
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
        "node_kind={} node_text={:?} current={:?} mapping={:?} context={:?}",
        action.kind(),
        action.utf8_text(source.as_bytes()).unwrap_or(""),
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 12).0,
        tracker.context_for_node(action)
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
            .output_context_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.output_path.0,
            want: vec!["env[*]", "value"],
            "node kind {}",
            node.kind()
        );
        assert!(
            context.entire_scalar_value,
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
            .output_context_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.output_path.0,
            want: vec!["ports[*]", "port"],
            "node kind {}",
            node.kind()
        );
        assert!(
            context.entire_scalar_value,
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .path
            .0,
        want: vec!["ports[*]", "port"]
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .path
            .0,
        want: vec!["ports[*]", "port"]
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .path
            .0,
        want: vec!["env[*]", "valueFrom", "secretKeyRef", "name",]
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
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
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 18).0,
        tracker.context_for_node(action)
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let rendered_context = tracker
        .attribution
        .virtual_indent_context_for_node(action, 4);
    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(4))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "ingress"],
        "current={:?} mapping={:?} context={:?} rendered={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 4).0,
        tracker.context_for_node(action),
        rendered_context
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(4))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["metadata", "labels"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 4).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    let path = tracker
        .output_slot_for_node(action, ValueKind::Fragment, Some(6))
        .path;
    sim_assert_eq!(
        have: path.0,
        want: vec!["spec", "podSelector", "matchLabels"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 6).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker.path_for_node(action).0,
        want: vec![
            "spec",
            "ingress[*]",
            "from[*]",
            "namespaceSelector",
            "matchLabels",
        ],
        "mapping={:?} context={:?}",
        tracker.path_at_mapping_entry_indent(action, 16).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker.path_for_node(action).0,
        want: vec![
            "spec",
            "ingress[*]",
            "from[*]",
            "namespaceSelector",
            "matchLabels",
        ],
        "mapping={:?} context={:?}",
        tracker.path_at_mapping_entry_indent(action, 16).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), range);

    sim_assert_eq!(
        have: tracker.path_at_mapping_entry_indent(range, 16).0,
        want: vec![
            "spec",
            "ingress[*]",
            "from[*]",
            "namespaceSelector",
            "matchLabels",
        ],
        "context={:?}",
        tracker.context_for_node(range)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .path
            .0,
        want: vec!["metadata", "name"],
        "current={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.context_for_node(action)
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);
    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .path
            .0,
        want: vec!["spec", "volumeClaimTemplates[*]", "spec"],
        "current={:?} mapping={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.path_at_mapping_entry_indent(action, 8).0,
        tracker.context_for_node(action)
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .path
            .0,
        want: vec!["spec", "ingress[*]", "ports[*]", "port"]
    );
    assert!(
        tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value,
        "current={:?} context={:?}",
        tracker.path_for_node(action).0,
        tracker.context_for_node(action).output_path.0
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
    drive_tracker_until(&mut tracker, tree.root_node(), action);

    assert!(tracker.context_for_node(action).inside_block_scalar);
    sim_assert_eq!(
        have: tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .path
            .0,
        want: Vec::<String>::new()
    );
    assert!(
        !tracker
            .output_slot_for_node(action, ValueKind::Scalar, None)
            .entire_scalar_value
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
            .output_context_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.output_path.0,
            want: vec!["env[*]", "value"],
            "node kind {}",
            node.kind()
        );
        assert!(
            context.entire_scalar_value,
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
            .output_context_for_node(node)
            .unwrap_or_else(|| panic!("missing context for node kind {}", node.kind()));
        sim_assert_eq!(
            have: context.output_path.0,
            want: vec!["args[*]"],
            "node kind {}",
            node.kind()
        );
        assert!(
            !context.entire_scalar_value,
            "node kind {} should be embedded in the scalar value",
            node.kind()
        );
    }
}

fn drive_tracker_until(
    tracker: &mut DocumentTracker<'_>,
    node: tree_sitter::Node<'_>,
    target: tree_sitter::Node<'_>,
) -> bool {
    if node.id() == target.id() {
        return true;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if drive_tracker_until(tracker, child, target) {
            return true;
        }
    }
    false
}

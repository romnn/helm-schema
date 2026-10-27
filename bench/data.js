window.BENCHMARK_DATA = {
  "lastUpdate": 1793107827990,
  "repoUrl": "https://github.com/romnn/helm-schema",
  "entries": {
    "helm-schema end-to-end": [
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "a0141424e6764fc6e8143f8b075407c7bc1f4dae",
          "message": "chore(plan): record landing 10a run 3, the benchmarks merge and the typos repair",
          "timestamp": "2026-10-20T16:14:48+02:00",
          "tree_id": "2674aff975a2dce0dd4a1b238e53e8fe5989724d",
          "url": "https://github.com/romnn/helm-schema/commit/a0141424e6764fc6e8143f8b075407c7bc1f4dae"
        },
        "date": 1792506676322,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full cpu",
            "value": 0.1027,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.103,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0181,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0012,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0749,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.006,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0111,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0045,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.113 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.9626,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.9636,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 319,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1267,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4383,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.2242,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2382,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.3348,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.1477,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.915-5.073 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1876,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.1883,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 260.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1148,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4466,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.5825,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2373,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0847,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0087,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.114-3.213 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 42.8185,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 42.8246,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1885.8,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 10719.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.3645,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.8019,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 33.7021,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 4.6072,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 21.142,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.689,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 42.485-43.042 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 24.2257,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 24.2306,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1375,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.3515,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.7897,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.7709,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 4.5771,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 8.0481,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.5142,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.127-25.048 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 27.582,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 27.588,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1336.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 5401.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.8944,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.4592,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.0368,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 4.0128,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 12.2238,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.6883,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.569-27.677 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "acf3146bc6ef4cfb7ad19c488f4a976fe80b3796",
          "message": "chore(plan): fix the W1 slice 2 entry's timestamp",
          "timestamp": "2026-10-20T18:11:17+02:00",
          "tree_id": "49542bb02282d6791d436c4611cc78e8bc5d4a90",
          "url": "https://github.com/romnn/helm-schema/commit/acf3146bc6ef4cfb7ad19c488f4a976fe80b3796"
        },
        "date": 1792515983082,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full cpu",
            "value": 0.1049,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1056,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0175,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0013,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0776,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0063,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0114,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0045,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.101-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 5.2791,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 5.2803,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 318.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1446,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.5336,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.3414,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2476,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.391,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.1791,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 5.219-5.454 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.4161,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.4167,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 260,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.2247,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4748,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6132,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2522,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0869,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0096,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.324-3.460 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 43.8093,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 43.8181,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1885.8,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 10719.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.5482,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.9882,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 34.8775,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 4.9457,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 21.6906,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.7459,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 43.157-45.143 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 24.6032,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 24.6066,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1375,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.4502,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.8766,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.5789,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 4.5985,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 7.7992,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.5452,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.544-24.791 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 28.3426,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 28.3461,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1339.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 5401.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.2435,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.8138,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 21.3482,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 4.2205,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 12.9369,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.7754,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.135-28.776 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "5c5d0e9f00673cac2293c9e3032e299ab281fbf6",
          "message": "fix(bench): pair Perfetto slice ends with their begin per track\n\nThe romnn/tracing-perfetto fork (7de0d96f) writes standard Perfetto: a span's\nSLICE_BEGIN when it is entered and a nameless SLICE_END when it closes, so nested\nspans interleave. bench.py assumed the old crate's layout (each begin followed by\nits own named end) and failed on main's bench run 39487437055 with 'span end None\nwithout its begin'. Ends now pop the most recent open begin on the same track_uuid;\nan end without a begin or a span never closed still refuses.\n\nVerified locally: span_intervals on a fresh nats trace pairs all 62 span names\nwith every PHASES entry present; tools/bench/bench.py --chart kubeview --reps 3\nexits 0 with 12 entries.",
          "timestamp": "2026-10-20T22:07:25+02:00",
          "tree_id": "38ee6e4872c9aa38b0dd52cf1415f8752436ab72",
          "url": "https://github.com/romnn/helm-schema/commit/5c5d0e9f00673cac2293c9e3032e299ab281fbf6"
        },
        "date": 1792527151975,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full cpu",
            "value": 0.0589,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0593,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0112,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.001,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0484,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0052,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0045,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0059,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.057-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 3.0605,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 3.0611,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 322,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 1.4432,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4261,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.2204,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.1907,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.5464,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2081,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.013-3.251 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 2.0593,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 2.1099,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 262.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 1.3208,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.3778,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.4142,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.1795,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0329,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0096,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.989-2.139 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 22.0625,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 22.072,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1881.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 2.6808,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.0554,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 18.1308,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 3.5397,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 8.2508,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.5948,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.639-22.263 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 14.4042,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 14.4186,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1377.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 2.572,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.0347,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 10.8855,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 3.4774,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 3.5289,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.5006,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.154-14.452 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 15.006,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 15.0082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1366.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 2.3077,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 1.9328,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 11.8089,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 3.0447,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 5.2336,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.7079,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.893-15.375 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "4b2359d0735eaded685d9580c85c70ad247e7f0f",
          "message": "Merge fix/windows-safe-paths (cc991d97): drop the two Windows-invalid provider-bundle records\n\ncertificate-\".json.not-found and issuer-\".json.not-found made every windows-latest CI job\nfail at checkout. Byte-identity check: one corpus producer dump of the branch (205 artifacts,\nproducer exit 0) compared with every committed fixture: 0 of 204 changed. The root-cause fix\n(mis-parsed apiVersion, cache writers refusing non-portable names, a repo-wide Windows path\ntest) follows on the same branch.",
          "timestamp": "2026-10-20T22:09:03+02:00",
          "tree_id": "49ad56352d9ce54009f9ad06df7cc460ccdc402b",
          "url": "https://github.com/romnn/helm-schema/commit/4b2359d0735eaded685d9580c85c70ad247e7f0f"
        },
        "date": 1792527746804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full cpu",
            "value": 0.1017,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.102,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0169,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0836,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0085,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.008,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0098,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.109 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.489,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.4899,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 321.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.0827,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4914,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3163,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.9897,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2425,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.478-4.521 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1557,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.1566,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 258,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.0969,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4955,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.7035,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3178,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0608,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0172,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.151-3.166 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 35.317,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 35.3219,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1884.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.4173,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 3.0497,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 29.0828,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.73,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.5511,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.3107,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.256-35.375 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 22.0547,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 22.0584,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1373.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.3002,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 3.0611,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.8064,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.7166,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.9301,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.7079,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.013-22.162 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 24.1738,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 24.1771,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1344.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.9387,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.8622,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.5109,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.6763,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 8.9915,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.9943,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.107-24.199 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "98b1ebd02e6331e3ab407c05a5db92ff0da5c0bd",
          "message": "fix(task): run helmsweep's stage.sh through sh so Windows can build it\n\nThe Windows integration job (first to get past checkout after 4b2359d0) failed in\nbuild:helmsweep with 'mise ERROR %1 is not a valid Win32 application': mise exec\nstarted ./stage.sh as a native program. Invoking it as 'sh ./stage.sh' works on\nevery runner (Git for Windows ships sh). task build:helmsweep exit 0 on macOS.",
          "timestamp": "2026-10-20T23:18:32+02:00",
          "tree_id": "008e1d0f90d1087bf498a378a36317f2dcdb01f6",
          "url": "https://github.com/romnn/helm-schema/commit/98b1ebd02e6331e3ab407c05a5db92ff0da5c0bd"
        },
        "date": 1792531502941,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full cpu",
            "value": 0.08,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0804,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.014,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0012,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0663,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0067,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0061,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0075,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.078-0.086 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 3.7504,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 3.751,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 323.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 1.6966,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4343,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.672,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2513,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.7988,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2159,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.741-3.773 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 2.643,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 2.6436,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 254.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 1.6782,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4396,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.5947,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2484,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0487,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0132,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.628-2.703 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 29.4922,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 29.4952,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1887.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 3.4567,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.5525,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 23.896,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 4.561,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 11.066,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.0263,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 29.354-29.947 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 18.3444,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 18.3475,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1379.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 3.4538,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.535,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 13.8672,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 4.5677,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.1937,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6414,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 18.188-18.453 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 19.8662,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 19.8682,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1368.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.2337,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.3513,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 16.3667,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 4.3533,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 7.3345,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.8699,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.828-19.998 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "bd54428e10f655445d98cb7c8aefdfe0f96700e1",
          "message": "fix(test-support): re-pin the Kubernetes provider bundle digest after dropping the two Windows-invalid records\n\n4b2359d0 deleted certificate-\".json.not-found and issuer-\".json.not-found, which changed the\ncommitted bundle's content identity; cell_matrix refused the bundle on every platform\n(13 tests; CI integration jobs). New pin af56b70f… is the digest both the local clean tree and\nCI's integration (ubuntu-24.04) job computed; cell_matrix 14/14 pass.",
          "timestamp": "2026-10-20T23:37:17+02:00",
          "tree_id": "1420be476c7f8893fe41c87c07bae2caa65c2b69",
          "url": "https://github.com/romnn/helm-schema/commit/bd54428e10f655445d98cb7c8aefdfe0f96700e1"
        },
        "date": 1792532529871,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full cpu",
            "value": 0.059,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.12,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0105,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.001,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0488,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0052,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0052,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0057,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.058-0.066 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 2.7637,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 2.7645,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 320.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 1.2517,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.3407,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.1669,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.1818,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.5453,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.159,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.735-2.885 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 1.9287,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 1.9843,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 262.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 1.2426,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.3444,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.3994,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.1769,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.033,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0102,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.907-1.990 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 21.0519,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 21.0551,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1897.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 2.5498,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 1.9677,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 16.8744,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 3.1375,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 7.644,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.5786,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.036-21.170 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 13.2538,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 13.2558,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1383,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 2.4974,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 1.9356,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 9.8702,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 3.2249,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 2.946,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.4563,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.000-13.298 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 14.1953,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 14.1973,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1345.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 2.206,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 1.7788,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 11.4473,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 3.0223,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 4.9363,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.6714,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 14.122-14.259 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "e6e4c5845ec458d2c839f1c87d8636809f0cfcea",
          "message": "Merge feat/bench-cpu-and-instructions: benchmark per runner CPU, cachegrind instruction counts\n\nbench.py records the runner CPU model; pages.yaml pins benchtrail v0.4.0 and splits the\nend-to-end times into one series per CPU (GitHub's ubuntu-24.04 pool mixes EPYC 7763/9V74/9V45,\nwhich moved CPU times 20-50% between pushes without code changes); a new 'helm-schema\ninstructions' suite counts instructions under pinned valgrind cachegrind for kubeview and\nbitnami-redis full (spread < 0.1%, est. +2-4.5 min CI). ADR 0068; bench wording says every\npush, not every commit. Opus subagent; local bench.py exit 0, Linux container cachegrind run\nexit 0, actionlint 0, typos 0; benchtrail 83 tests.",
          "timestamp": "2026-10-20T23:46:39+02:00",
          "tree_id": "6d5121fe28c881b2300b0e252541b3b75a73b78f",
          "url": "https://github.com/romnn/helm-schema/commit/e6e4c5845ec458d2c839f1c87d8636809f0cfcea"
        },
        "date": 1792533357916,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full cpu",
            "value": 0.0996,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0999,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0215,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0807,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0083,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0081,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0089,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.4832,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.4842,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 321.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1427,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4549,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.9941,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3005,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0282,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2294,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.439-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1192,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.12,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 260,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1311,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4475,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6548,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3004,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0605,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0152,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.098-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 33.5947,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 33.5999,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1890.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.3889,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.8444,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 27.72,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.4083,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.3286,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.0347,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.428-33.679 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 21.0437,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 21.0465,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1374.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.388,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.8714,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 15.7768,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.404,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.7158,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6437,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.950-21.135 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 23.2894,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 23.2938,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1365.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.0901,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.5752,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 19.763,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.5171,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.0362,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.8991,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.282-23.527 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full instructions",
            "value": 604661744,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27780152544,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "7dc5ed9e25c45fc53207072c05da52ffe79e646f",
          "message": "Merge feat/bench-one-suite: one benchmark tab, instructions first, last 50 runs, short CPU names\n\nbenchtrail v0.5.0 (2d439907): at most two charts per row; per-metric settings so the times split by\nrunner CPU while the instruction counts stay one series and come first; lineLabel 'full (EPYC 7763)';\ndefault range last 50 runs; bench.yaml keeps at most 500 runs per suite (max-items-in-chart).\nbench.py records one suite and short CPU names (vendor and core-count noise removed, model kept;\ntested). gh-pages history migrated once (instruction suite folded in). Opus subagent; actionlint 0,\ntypos 0, bench unit tests 0, benchtrail lint/check/test 0.",
          "timestamp": "2026-10-21T00:45:24+02:00",
          "tree_id": "7983fd5a52fad333d37469a4194f2e940ac9515b",
          "url": "https://github.com/romnn/helm-schema/commit/7dc5ed9e25c45fc53207072c05da52ffe79e646f"
        },
        "date": 1792536931763,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 604698454,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.1053,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1056,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0227,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0826,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0087,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0085,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.009,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.097-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27775293937,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.6122,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.6128,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 321.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.2684,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.5216,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.168,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3235,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0742,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2801,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.606-4.819 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1819,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.1825,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 258.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1542,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4978,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6829,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3113,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0619,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0155,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.161-3.196 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 34.771,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 34.7746,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1888,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.8034,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 3.2056,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 32.1543,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 6.6981,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 15.0413,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.361,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.188-36.962 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 25.344,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 25.3471,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1373.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.9237,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 3.2038,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 19.2908,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 6.9047,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.6646,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.7539,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.221-25.585 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 25.8638,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 25.8681,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1342,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.4523,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.9744,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 21.5854,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.9149,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.7194,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 1.0593,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 25.794-26.230 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "b89ed37dc45396de43d77e03bf00effd724e45d4",
          "message": "ci(go): adopt golangci-lint for the Go tooling\n\n.golangci.yaml (v2) from luup2's config: errcheck, errorlint, gosec, govet, staticcheck, unused, and\nothers; goimports formatter. golangci-lint 2.14.0 pinned in mise. task lint:go lints the stage\nprogram from source and helmsweep in its staged copy (the bare tree only compiles after the Helm\npatch), replacing the hand-maintained gofmt -l / go vet steps; test:helmsweep and task lint use it.\nFixed: errorlint 2, gosec 5, ineffassign 1. Caller-supplied local paths and readable report modes\nin the maintainer CLI are excluded by one documented rule; 9 specific nolints keep their reasons.\nVerified on main: task lint:go 0 issues, task build:helmsweep 0, task test:helmsweep 0, actionlint 0,\ntypos 0. codex sol 20261020T215728-0ed3a2b6, patch sha256 fb1864e4…",
          "timestamp": "2026-10-21T00:57:32+02:00",
          "tree_id": "f4d752af6b3d87c95e384800173503abdf578c57",
          "url": "https://github.com/romnn/helm-schema/commit/b89ed37dc45396de43d77e03bf00effd724e45d4"
        },
        "date": 1792537615659,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 604610838,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.0983,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0986,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0183,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0795,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0083,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.008,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0089,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.105 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27779387547,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.3979,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.4347,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 322.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4394,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.9152,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2963,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.9906,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2041,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.356-4.458 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.0975,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.098,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 262.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1257,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4355,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6498,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2983,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0591,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0154,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.067-3.134 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 33.8108,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 33.8145,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1886.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.5556,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.9178,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 28.1934,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.7123,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.3867,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.0723,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.528-34.674 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 21.4098,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 21.4125,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1375.8,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.412,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.9606,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.4067,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.4081,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6692,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.257-21.442 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 23.7172,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 23.7207,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1346.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.0696,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.7101,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 19.7313,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.5011,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 8.9576,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.8724,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.699-23.757 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "429eba5175a1c69284d110336576b169f995e29c",
          "message": "chore(plan): record landing 10b merged into main",
          "timestamp": "2026-10-21T01:52:35+02:00",
          "tree_id": "b981a7027a534be2277d187f42ddee56715aa9dd",
          "url": "https://github.com/romnn/helm-schema/commit/429eba5175a1c69284d110336576b169f995e29c"
        },
        "date": 1792547268734,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 604457984,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.054,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0542,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0109,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0013,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0475,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0049,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0041,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0054,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 0.053-0.057 s over 9 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27765005784,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 2.5041,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 2.5047,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 320.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 1.2036,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.3082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.0591,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.1676,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.4965,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.1263,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 2.498-2.509 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 1.7988,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 1.7992,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 261.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 1.1756,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.3106,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.3775,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.1691,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0312,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0092,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 1.795-1.804 s over 5 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 19.6386,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 19.6463,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1882.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 2.2986,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 1.8113,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 15.7473,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 2.9834,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 7.047,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.4419,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 19.549-19.647 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 12.2237,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 12.2271,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1375.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 2.3226,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 1.82,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 9.1986,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 2.9639,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 2.7272,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.4091,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 12.146-12.343 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 13.279,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 13.2807,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1392.8,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 2.0939,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 1.6777,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 10.8514,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 2.9407,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 4.6066,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.6023,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V45\"}},\"text\":\"cpu 13.248-13.281 s over 3 runs; AMD EPYC 9V45 96-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "8a2f3ce124649bd3edda7ac98996caf122e49525",
          "message": "chore(plan): record landing 10c assembly, its reviews and the Codex track hand-overs",
          "timestamp": "2026-10-21T13:02:10+02:00",
          "tree_id": "f361feb17eb136d939702f1d0c8c8155ad667385",
          "url": "https://github.com/romnn/helm-schema/commit/8a2f3ce124649bd3edda7ac98996caf122e49525"
        },
        "date": 1792582756538,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 604655429,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.1105,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1109,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0202,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0021,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0882,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0085,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.009,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.103-0.123 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27771036926,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.8403,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.8414,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 325.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1672,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.5017,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.0196,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3109,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0125,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2499,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.823-4.937 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1758,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.1765,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 258.8,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1289,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4984,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6753,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3102,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0598,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0151,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.112-3.254 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 34.2111,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 34.2149,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1887.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.4044,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.9121,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 27.8992,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.4155,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.3263,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.1167,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.749-34.547 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 21.44,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 21.4429,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1377.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.5832,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 3.0647,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.8773,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.8058,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.1303,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.7006,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.282-22.692 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 24.9522,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 24.9549,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1338.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.1916,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.8127,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.3432,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.6274,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.272,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.951,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.698-25.024 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "7c20061888db345ff334b1af256f4a6ee1c09039",
          "message": "Merge landing 10c: lint-visible required keys, corpus-wide partial-override lane, Windows root cause, dev binary\n\nComponents (landing-10c, 4d2baced, on 429eba51):\n- fix(gen) d3cf7a2f: mandatory constraints are checked against lint documents (pre-pass deleted):\n  shipped schemas no longer require keys only a subchart default provides, which made `helm lint`\n  fail for umbrella charts while `helm template` rendered (luup2 5/32 -> 32/32; introduced by\n  83247085). Coverage: corpus-wide partial-override lint lane for every dependency (71f062f0),\n  243-cell Helm-differential matrix + 156 unguarded feature-path cells + 18 default-satisfied\n  requirement cells (15709a22, 85934c94), daskhub 2024.1.1 public umbrella intake (f51b369a),\n  80 exact pre-existing inherited entries the lane exposes (9d26c1e1; reviewed pre-existing).\n- fix/windows-safe-paths (f9fb24a5): quoted templated apiVersion parse, invalid apiVersions never\n  looked up, cache writers refuse non-portable names (warn + skip), repo-wide Windows path test.\n- refactor 02e4beca: tracing-perfetto lives only in the unpublished helm-schema-dev binary.\n\nReviews: sol 20261021T055322-03835967 LAND (ratchet audit), sol 20261021T055321-7d80fb9c REWORK\n(test coverage) -> 85934c94; devbin sol 20261020T225802-1068817c (skill edits applied).\nLanding chain run 2 (runner v11.2 296f3b1, env ef8f55e): dump 13:48-13:52 (206 verified, 0 adopted),\nunit, lint -14:01, battery -15:18 (coverage charts=161 flips_adjudicated=10156), integration -15:39,\nsweep -16:38, finalize 16:39.\nReceipt: /Volumes/T7/dev/round8-landing10c-run2/receipt.json\nsha256 66431ff3dcbb6bf77520f037e113c1ae386d8396019007d80ddeb9fda8fc3e42",
          "timestamp": "2026-10-21T16:55:12+02:00",
          "tree_id": "8dd3a4ca3325b6fb43171f6f3b00ff12b63f4a8b",
          "url": "https://github.com/romnn/helm-schema/commit/7c20061888db345ff334b1af256f4a6ee1c09039"
        },
        "date": 1792595232004,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 605304008,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.103,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1034,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0176,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0827,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0083,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0098,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.100-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27793529037,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.6728,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.6738,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 322.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1502,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 574.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 813,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.2164,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4886,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.0635,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3163,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0495,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2451,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.591-4.693 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.2542,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.2549,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 258.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.2001,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4993,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6967,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3101,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0652,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0157,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.226-3.258 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 34.771,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 34.7758,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1883.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.418,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.8471,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 28.0564,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.458,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.5284,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.049,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.787-35.155 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 22.2187,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 22.2216,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1364.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.5978,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.9456,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 17.0455,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.9454,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.0552,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6681,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.032-22.283 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 24.1705,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 24.1733,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1321.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1955.3,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2344,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.1862,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.621,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.148,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.6264,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.1563,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.9278,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.147-24.258 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "24938dd6f9faea306794c7aeddc4a07dcf750a7d",
          "message": "chore(plan): record the R0 design closure",
          "timestamp": "2026-10-22T08:33:40+02:00",
          "tree_id": "9db94a2b7d5946c4f2af6e28aef1ef9a0ab006b6",
          "url": "https://github.com/romnn/helm-schema/commit/24938dd6f9faea306794c7aeddc4a07dcf750a7d"
        },
        "date": 1792748896119,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 605256945,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.1022,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1025,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.019,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0812,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0083,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0091,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.110 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27880686033,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.6118,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.6128,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 322.8,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1575,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4718,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.0092,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3058,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0186,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.239,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.557-4.643 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1412,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.142,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 262.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1217,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4893,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6807,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3039,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0622,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0158,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.110-3.193 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 33.5175,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 33.5237,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1885.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.519,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.9966,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 28.8009,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.918,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.5436,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.0871,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.349-33.577 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 21.7595,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 21.7625,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1369.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.613,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.8857,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.3459,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.7466,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.8478,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6504,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.378-22.124 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 24.742,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 24.7456,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1312.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.1926,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.6184,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.4483,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.5984,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.4088,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.9487,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.521-24.857 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "0986c2b9fe3c025bc3666b1060a500e72e1cb881",
          "message": "chore(plan): give the inventory's landed list a header comment (no zero-byte file)",
          "timestamp": "2026-10-23T14:35:09+02:00",
          "tree_id": "a9f90c568f2e9dce5c53edc709714a563b7a364c",
          "url": "https://github.com/romnn/helm-schema/commit/0986c2b9fe3c025bc3666b1060a500e72e1cb881"
        },
        "date": 1792760180484,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 605243939,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.0975,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0979,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0184,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0799,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.009,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.104 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27876795497,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.4036,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.4065,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 321.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.0835,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4577,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.9663,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3047,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.9989,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2206,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.401-4.432 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.0954,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.0961,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 258.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.0806,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4375,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6667,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3029,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0605,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0156,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.080-3.144 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 33.5796,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 33.5842,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1883,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.3487,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.8884,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 28.3939,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.7416,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.4309,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.0686,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.504-33.655 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 21.3003,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 21.3026,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1371,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.4282,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.9142,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.1815,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.5685,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.9387,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6259,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.253-21.469 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 23.6158,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 23.6193,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1318.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.0086,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.6418,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 19.9127,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.5302,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.0962,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.9292,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.590-23.617 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "34f2c34ef593e414d16edc859f4cc7bf8b1770c9",
          "message": "chore(plan): session-6 hand-off draft and playbook model policy",
          "timestamp": "2026-10-24T11:49:50+02:00",
          "tree_id": "60a68c675f1e02f76b0cb7c8fa2ed44cd06beb1b",
          "url": "https://github.com/romnn/helm-schema/commit/34f2c34ef593e414d16edc859f4cc7bf8b1770c9"
        },
        "date": 1792836949408,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 605264788,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.0835,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0839,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 52.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0135,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0013,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0646,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0069,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0063,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0062,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.079-0.093 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27874889725,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 3.6636,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 3.6642,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 326.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 1.7966,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.3077,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.5815,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2349,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.7871,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.1783,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.637-3.716 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 2.5415,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 2.5419,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 254.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 1.7721,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.2938,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.5652,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2385,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0521,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.012,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 2.525-2.568 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 29.3595,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 29.3639,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1881.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 3.5731,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.3432,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 23.338,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 4.2378,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 11.5318,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.6918,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 28.610-29.438 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 17.8822,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 17.9247,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1371.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 3.4664,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.1979,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 12.6238,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 4.1597,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.1541,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.435,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 17.558-18.439 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 19.5856,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 19.5896,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1341.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.2064,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.0034,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 15.6174,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 3.9028,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 7.6631,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.6792,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 19.506-19.617 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "87dacd659ee39b88898b706e1ec86e46f454f3c6",
          "message": "chore(plan): playbook — parallel tracks and overlapping landings by default",
          "timestamp": "2026-10-24T18:04:14+02:00",
          "tree_id": "9c4da61808247e757f1790f34a828d786d260778",
          "url": "https://github.com/romnn/helm-schema/commit/87dacd659ee39b88898b706e1ec86e46f454f3c6"
        },
        "date": 1792859201222,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606960757,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.1014,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1017,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0184,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0083,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.009,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.099-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27948173643,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.6576,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.659,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 321.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1899,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4625,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.0298,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3175,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0167,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2518,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.622-4.724 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.2954,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.2961,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 256.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.2041,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4533,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.7228,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.323,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0637,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0157,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.287-3.315 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 34.4996,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 34.504,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1883.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.7198,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.9526,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 29.4901,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.8706,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 14.1302,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.1626,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.416-35.478 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 22.4581,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 22.4617,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1369.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.6884,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.9298,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.829,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.7541,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.1647,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6445,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.370-22.556 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 24.3096,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 24.3138,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1293.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.2129,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.6341,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.3142,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.6388,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.1502,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.948,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.288-24.487 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "86667e4c8748dc489ece821ce59e19677abc02d1",
          "message": "chore(plan): record landing 15 and the overnight R3b/perf/C08a decisions",
          "timestamp": "2026-10-26T11:57:11+01:00",
          "tree_id": "97d51ef9fc519087734111c4cde63149f850e7de",
          "url": "https://github.com/romnn/helm-schema/commit/86667e4c8748dc489ece821ce59e19677abc02d1"
        },
        "date": 1793026886612,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606856848,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0823,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0141,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0012,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0693,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0068,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0064,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0077,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.077-0.094 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27950366326,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 3.6558,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 3.6568,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 315.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 1.6738,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4098,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.6401,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2485,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.7905,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2155,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.646-3.783 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 2.5883,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 2.5891,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 253.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 1.6629,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4099,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.5732,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2461,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0486,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0133,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.580-2.657 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 28.6105,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 28.6142,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1883.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 3.4495,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.4872,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 23.5103,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 4.4712,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 10.8683,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.9424,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 28.518-29.062 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 18.0703,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 18.1126,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1370.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 3.4115,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.4535,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 13.9257,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 4.4486,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.3965,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6059,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.648-18.101 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 19.9028,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 19.9062,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1322.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.2276,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.2471,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 16.4959,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 4.4082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 7.3414,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.8753,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.651-19.982 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "c7cb0843b5bb4bcbb0033efe9d4ebd3ea58ab841",
          "message": "Merge landing 16: T0 value-algebra differential gate (test infrastructure)\n\nComponents: r3b/differential-tool at ba627bef (merge 427e44a1): value_algebra_diff capture/init/compare/verify with a\nsingle-writer atomic history, stable witness identities, required context roster, grounded controls; reviews\n20261025T030526-1760fdd7 (REWORK x2 -> LAND).\n\nChain: /Volumes/T7/dev/round8-landing16-run1 (env landing-16.env), all steps green, 0 fixtures adopted.\nReceipt: /Volumes/T7/dev/round8-landing16-run1/receipt.json sha256 aea1e04050c37e4187377a9b42174fdd21393c1e8781a778664faadf33d209e5",
          "timestamp": "2026-10-26T16:10:05+01:00",
          "tree_id": "8bbfb35a0b1d2f5248773d8ff08815d4d8818567",
          "url": "https://github.com/romnn/helm-schema/commit/c7cb0843b5bb4bcbb0033efe9d4ebd3ea58ab841"
        },
        "date": 1793029154443,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606923749,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.0975,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0977,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0246,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.002,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0847,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0089,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.095-0.112 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27943786224,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.4038,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.4045,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 317.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1434,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4311,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.9576,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2983,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0074,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2148,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.367-4.500 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1412,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.1419,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 256.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1463,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4467,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6669,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3003,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0626,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0158,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.137-3.148 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 33.7102,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 33.7171,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1882.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.4644,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.7733,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 27.5055,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.2924,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.3274,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.0468,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 33.427-33.972 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 20.9184,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 20.9228,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1366.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.5677,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.8423,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 15.5385,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.265,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.6794,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6278,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 20.748-22.313 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 22.9909,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 22.9935,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1322.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.0143,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.4835,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 19.5324,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.4745,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 8.8985,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.9138,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 22.935-23.113 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "e04ff166f648591ab6e0ce9b4522ee4e675d2071",
          "message": "chore(plan): record landing 18, C04a and the parallel perf lane",
          "timestamp": "2026-10-26T22:08:28+01:00",
          "tree_id": "9d13b0e067793e28720e6c2066ea5b1cd4446b79",
          "url": "https://github.com/romnn/helm-schema/commit/e04ff166f648591ab6e0ce9b4522ee4e675d2071"
        },
        "date": 1793050653487,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606612041,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.1001,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1007,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0166,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0796,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0077,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0073,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 0.099-0.102 s over 9 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27951759019,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.5385,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.5414,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 320.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1595,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.4452,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.9094,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2763,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.9534,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2249,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 4.527-4.542 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.1973,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.1982,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 258.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1841,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4522,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6443,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2781,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0625,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0133,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 3.172-3.220 s over 5 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 36.0521,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 36.0583,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1881.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.2916,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.7425,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 28.2523,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.2125,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 14.1905,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.9784,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 35.843-36.083 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 22.5567,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 22.5889,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1366.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.3122,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.7271,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 16.609,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.4521,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.8711,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6051,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 22.456-22.638 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 23.9158,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 23.9193,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1338.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.9525,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.4993,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 18.8147,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 4.5953,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.4273,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.8381,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8573C\"}},\"text\":\"cpu 23.872-23.951 s over 3 runs; INTEL(R) XEON(R) PLATINUM 8573C; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "4cd6ed07c5505991e1342161ca1cb7bc0d129e38",
          "message": "chore(plan): record landing 19, the family re-tally and the C06a lane",
          "timestamp": "2026-10-27T03:37:07+01:00",
          "tree_id": "33f1f6ce414c8cd12baa9ad8b406d940953e25f1",
          "url": "https://github.com/romnn/helm-schema/commit/4cd6ed07c5505991e1342161ca1cb7bc0d129e38"
        },
        "date": 1793070026204,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606745919,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.0991,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0994,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0198,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0847,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0082,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0091,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.096-0.106 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27958320408,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.4735,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.4744,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 321.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.1517,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.447,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.975,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.299,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.0114,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2242,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.443-4.510 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.2345,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.2351,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 257,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.1889,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.4826,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6861,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3074,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0627,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0158,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.211-3.292 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 34.25,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 34.2589,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1882.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.5556,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.882,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 28.2416,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 5.4546,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.6278,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.0836,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 34.041-34.363 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 21.9458,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 21.9494,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1366.8,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.4906,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.8479,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 15.5914,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.3446,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.6424,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.6502,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 21.512-22.422 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 24.2639,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 24.267,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1310.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.2192,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.6338,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.2646,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.543,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.1802,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 1.0127,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.861-24.383 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "aa5147c350db34f68dc79e0fb3d9481070d6c720",
          "message": "chore(plan): record the R3b perf commit, C06a checkpoint and C04b deferral",
          "timestamp": "2026-10-27T06:31:49+01:00",
          "tree_id": "a3e8dc9de775fa2b3935a2290a6ff1b168f579ed",
          "url": "https://github.com/romnn/helm-schema/commit/aa5147c350db34f68dc79e0fb3d9481070d6c720"
        },
        "date": 1793096170726,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606560290,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.0765,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0769,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0139,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0011,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0643,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0068,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0063,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0076,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 0.075-0.082 s over 9 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27937358135,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 3.6038,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 3.6045,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 324.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 1.6514,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.3942,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.5987,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2419,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.7804,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.196,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 3.576-3.619 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 2.5208,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 2.5215,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 254.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 1.655,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.392,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.559,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.242,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0483,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.013,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 2.510-2.524 s over 5 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 27.8155,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 27.8194,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1886.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 3.4626,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.4191,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 23.1282,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 4.4306,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 10.7763,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.8926,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 27.813-27.829 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 17.2968,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 17.3085,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1371.3,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 3.4456,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.4058,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 13.219,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 4.3259,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 4.015,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.5716,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 17.297-17.305 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 19.065,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 19.0677,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1339.7,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.1423,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.1896,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 15.8893,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 4.3195,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 7.0418,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.7913,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 9V74\"}},\"text\":\"cpu 19.055-19.166 s over 3 runs; AMD EPYC 9V74 80-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "a2c8e1861f2f5a64e9203c9b4fa2d5caeef8cce5",
          "message": "Merge bench: CPU-independent metrics keep one series across runners\n\nComponents: 467e1c2c fix(bench): omit cpu environment from memory, sizes and counts; pin bump to\nbenchtrail v0.5.1 (2b1efb0c, romnn/benchtrail fix 2bb7b89). Tooling only (tools/bench, workflows,\nADR 0068); no generation input changes, so no landing chain. Builder sol 20261027T113424-7dd66cc1:\npython 4 passed, task lint and lint:actions passed; benchtrail task test 94 passed.",
          "timestamp": "2026-10-27T13:28:20+01:00",
          "tree_id": "a35788d5318aa44b67b8c12db7073eb96aa8de8b",
          "url": "https://github.com/romnn/helm-schema/commit/a2c8e1861f2f5a64e9203c9b4fa2d5caeef8cce5"
        },
        "date": 1793105248241,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606728510,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.0965,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.0969,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.016,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0014,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0774,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0076,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.0075,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0073,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 0.094-0.111 s over 9 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27945673715,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.321,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.3218,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 317.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.0586,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.385,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 1.8679,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.2635,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 0.964,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.2093,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 4.263-4.366 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.0195,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.0202,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 262.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.084,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.3892,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.6108,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.2634,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0591,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0128,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 3.009-3.034 s over 5 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 32.9628,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 32.9685,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1882.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.1576,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 2.4921,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 26.1518,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 4.8111,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 13.221,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 1.7681,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 32.948-33.403 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 20.9651,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 20.9675,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1366.1,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.2744,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 2.5459,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 15.6032,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 5.1874,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.4252,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.5267,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 20.814-21.791 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 22.5192,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 22.5212,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1312.5,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 3.8639,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.3156,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 18.296,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 4.564,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.1506,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.7649,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"Xeon Platinum 8370C\"}},\"text\":\"cpu 22.337-22.533 s over 3 runs; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "committer": {
            "email": "roman.dahm@gmx.de",
            "name": "romnn",
            "username": "romnn"
          },
          "distinct": true,
          "id": "02000db1216f1cebbd138c589a98e30825a76cff",
          "message": "Merge bench: case-insensitive CPU name normalization",
          "timestamp": "2026-10-27T14:20:14+01:00",
          "tree_id": "e588b2f19f00771ace4946ec38942ccc47171fa6",
          "url": "https://github.com/romnn/helm-schema/commit/02000db1216f1cebbd138c589a98e30825a76cff"
        },
        "date": 1793107824880,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "kubeview/full instructions",
            "value": 606789065,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full cpu",
            "value": 0.1005,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full wall",
            "value": 0.1008,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full peak-rss",
            "value": 50.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full schema-size",
            "value": 173.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full compact-size",
            "value": 97,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full defs",
            "value": 50,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:analyze_charts",
            "value": 0.0203,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:finalize",
            "value": 0.0016,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:generate_values_schema_with_report",
            "value": 0.0956,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:collect_conditional_schemas",
            "value": 0.0087,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:append_selected_constraints",
            "value": 0.009,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kubeview/full phase:minimize_schema",
            "value": 0.0097,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kubeview\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 0.098-0.111 s over 9 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full instructions",
            "value": 27950644288,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"instructions\",\"label\":\"Instructions (cachegrind)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"valgrind\":\"valgrind-3.22.0\"}},\"text\":\"one cachegrind run; valgrind-3.22.0; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full cpu",
            "value": 4.8524,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full wall",
            "value": 4.8537,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full peak-rss",
            "value": 321.6,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full schema-size",
            "value": 1510.7,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full compact-size",
            "value": 577.6,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full defs",
            "value": 825,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:analyze_charts",
            "value": 2.2999,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:finalize",
            "value": 0.5096,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:generate_values_schema_with_report",
            "value": 2.1847,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:collect_conditional_schemas",
            "value": 0.3343,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:append_selected_constraints",
            "value": 1.097,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/full phase:minimize_schema",
            "value": 0.262,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 4.784-4.928 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean cpu",
            "value": 3.3982,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean wall",
            "value": 3.3988,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean peak-rss",
            "value": 259.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean schema-size",
            "value": 175.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean compact-size",
            "value": 89.8,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean defs",
            "value": 81,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:analyze_charts",
            "value": 2.2683,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:finalize",
            "value": 0.5147,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:generate_values_schema_with_report",
            "value": 0.7235,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:collect_conditional_schemas",
            "value": 0.3248,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:append_selected_constraints",
            "value": 0.0656,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "bitnami-redis/lean phase:minimize_schema",
            "value": 0.0163,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"bitnami-redis\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 3.340-3.430 s over 5 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full cpu",
            "value": 36.7912,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full wall",
            "value": 36.7957,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full peak-rss",
            "value": 1879.4,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full schema-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full compact-size",
            "value": 3875.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full defs",
            "value": 4360,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:analyze_charts",
            "value": 4.7372,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:finalize",
            "value": 3.0404,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:generate_values_schema_with_report",
            "value": 30.41,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:collect_conditional_schemas",
            "value": 6.1251,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:append_selected_constraints",
            "value": 14.3753,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/full phase:minimize_schema",
            "value": 2.2201,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"full\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 36.032-36.827 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean cpu",
            "value": 23.5316,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean wall",
            "value": 23.5347,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean peak-rss",
            "value": 1366.9,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean schema-size",
            "value": 4357.9,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean compact-size",
            "value": 1772.4,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean defs",
            "value": 1918,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:analyze_charts",
            "value": 4.7997,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:finalize",
            "value": 3.0946,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:generate_values_schema_with_report",
            "value": 17.5689,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:collect_conditional_schemas",
            "value": 6.0625,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:append_selected_constraints",
            "value": 5.3296,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "kube-prometheus-stack/lean phase:minimize_schema",
            "value": 0.7114,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"kube-prometheus-stack\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 23.401-23.706 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean cpu",
            "value": 25.007,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"cpu\",\"label\":\"CPU time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean wall",
            "value": 25.0111,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"wall\",\"label\":\"Wall time\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean peak-rss",
            "value": 1317.2,
            "unit": "MiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"peak-rss\",\"label\":\"Peak RSS\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean schema-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"schema-size\",\"label\":\"Schema size\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean compact-size",
            "value": 1956.1,
            "unit": "KiB",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"compact-size\",\"label\":\"Compact schema size (short $defs)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean defs",
            "value": 2345,
            "unit": "count",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"defs\",\"label\":\"$defs entries\",\"direction\":\"unknown\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:analyze_charts",
            "value": 4.3748,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:analyze_charts\",\"label\":\"Phase: template analysis (analyze_charts)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:finalize",
            "value": 2.6797,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:finalize\",\"label\":\"Phase: contract finalization (finalize)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:generate_values_schema_with_report",
            "value": 20.5088,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:generate_values_schema_with_report\",\"label\":\"Phase: schema generation (generate_values_schema_with_report)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:collect_conditional_schemas",
            "value": 5.7408,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:collect_conditional_schemas\",\"label\":\"Phase: conditional schemas (collect_conditional_schemas)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:append_selected_constraints",
            "value": 9.2821,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:append_selected_constraints\",\"label\":\"Phase: selected constraints (append_selected_constraints)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          },
          {
            "name": "oncall/lean phase:minimize_schema",
            "value": 0.9689,
            "unit": "s",
            "extra": "{\"benchtrail\":{\"version\":1,\"dimensions\":{\"chart\":\"oncall\",\"profile\":\"lean\"},\"metric\":{\"id\":\"phase:minimize_schema\",\"label\":\"Phase: minimization (minimize_schema)\",\"direction\":\"lower\"},\"environment\":{\"rustc\":\"rustc 1.99.0 (b940084d7 2026-09-28)\",\"cpu\":\"EPYC 7763\"}},\"text\":\"cpu 24.814-25.074 s over 3 runs; AMD EPYC 7763 64-Core Processor; rustc 1.99.0 (b940084d7 2026-09-28)\"}"
          }
        ]
      }
    ]
  }
}
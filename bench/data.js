window.BENCHMARK_DATA = {
  "lastUpdate": 1792506680476,
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
      }
    ]
  }
}
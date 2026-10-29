"""Tests of bench.py's pure helpers: python3 -m unittest discover tools/bench"""

import json
import unittest
from unittest.mock import patch

from bench import PHASES, entries, short_cpu


class ShortCpu(unittest.TestCase):
    def test_drops_only_the_noise_of_known_vendors(self) -> None:
        cases = {
            "AMD EPYC 7763 64-Core Processor": "EPYC 7763",
            "AMD EPYC 9V74 80-Core Processor": "EPYC 9V74",
            "AMD EPYC 9V45 96-Core Processor": "EPYC 9V45",
            "Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz": "Xeon Platinum 8370C",
            "INTEL(R) XEON(R) PLATINUM 8573C": "Xeon Platinum 8573C",
            "INTEL(R) XEON(R) PLATINUM 8370C CPU @ 2.80GHZ": "Xeon Platinum 8370C",
            "AMD EPYC 7763 64-CORE PROCESSOR": "EPYC 7763",
            "APPLE M3 PRO": "M3 Pro",
            "AMD EPYC 7763": "EPYC 7763",
            "Apple M3 Pro": "M3 Pro",
            "  AMD   Ryzen 9  5950X 16-Core Processor ": "Ryzen 9 5950X",
        }
        for raw, short in cases.items():
            with self.subTest(raw=raw):
                self.assertEqual(short_cpu(raw), short)

    def test_keeps_what_it_does_not_recognize(self) -> None:
        # No vendor to drop, an unknown vendor, or nothing left: the name stays, whitespace collapsed.
        self.assertEqual(short_cpu(""), "")
        self.assertEqual(short_cpu("Neoverse-N1  r3p1"), "Neoverse-N1 r3p1")
        self.assertEqual(short_cpu("Ampere(R) Altra(R) Processor"), "Ampere(R) Altra(R) Processor")
        self.assertEqual(short_cpu("AMD"), "AMD")
        self.assertEqual(short_cpu("aarch64"), "aarch64")

    def test_different_models_keep_different_names(self) -> None:
        raws = [
            "AMD EPYC 7763 64-Core Processor",
            "AMD EPYC 7763",
            "AMD EPYC 9V74 80-Core Processor",
            "AMD EPYC 9V45 96-Core Processor",
            "AMD EPYC 7R13 Processor",
            "Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz",
            "Intel(R) Xeon(R) Platinum 8272CL CPU @ 2.60GHz",
            "Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz",
            "Apple M3 Pro",
            "Apple M3 Max",
            "Apple M3",
        ]
        models = {short_cpu(raw) for raw in raws}
        # The first two name one model, with and without its core count; every other name stays apart.
        self.assertEqual(len(models), len(raws) - 1)


class MetricEnvironment(unittest.TestCase):
    @patch("bench.CPU", "EPYC 7763")
    @patch("bench.RUSTC", "rustc test")
    def test_only_times_record_cpu_and_all_metrics_keep_their_toolchain(self) -> None:
        result = {
            "chart": "redis",
            "profile": "full",
            "reps": 3,
            "cpu_range": (1.0, 2.0),
            "cpu": 1.5,
            "wall": 2.0,
            "peak_rss": 1024 * 1024,
            "pretty": 2048,
            "compact": 1024,
            "defs": 3,
            "phases": dict.fromkeys(PHASES, 0.1),
            "instructions": 1000,
        }
        recorded = entries(result, "valgrind test")
        environments = {
            payload["metric"]["id"]: payload["environment"]
            for item in recorded
            for payload in [json.loads(item["extra"])["benchtrail"]]
        }
        time_environment = {"rustc": "rustc test", "cpu": "EPYC 7763"}
        shared_environment = {"rustc": "rustc test"}
        self.assertEqual(
            environments,
            {
                "cpu": time_environment,
                "wall": time_environment,
                **{f"phase:{phase}": time_environment for phase in PHASES},
                "peak-rss": shared_environment,
                "schema-size": shared_environment,
                "compact-size": shared_environment,
                "defs": shared_environment,
                "instructions": {**shared_environment, "valgrind": "valgrind test"},
            },
        )


if __name__ == "__main__":
    unittest.main()

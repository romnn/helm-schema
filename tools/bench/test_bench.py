"""Tests of bench.py's pure helpers: python3 -m unittest discover tools/bench"""

import unittest

from bench import short_cpu


class ShortCpu(unittest.TestCase):
    def test_drops_only_the_noise_of_known_vendors(self) -> None:
        cases = {
            "AMD EPYC 7763 64-Core Processor": "EPYC 7763",
            "AMD EPYC 9V74 80-Core Processor": "EPYC 9V74",
            "AMD EPYC 9V45 96-Core Processor": "EPYC 9V45",
            "Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz": "Xeon Platinum 8370C",
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


if __name__ == "__main__":
    unittest.main()

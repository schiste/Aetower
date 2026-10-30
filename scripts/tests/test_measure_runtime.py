"""Counter-based runtime measurements must exclude pre-profile CPU/wakeups."""
import importlib.util
import os
from pathlib import Path
import resource
import sys
import unittest


SPEC = importlib.util.spec_from_file_location(
    "measure_runtime", Path(__file__).resolve().parents[1] / "measure-runtime.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RuntimeMeasurementTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "darwin", "requires macOS libproc")
    def test_live_cpu_units_match_getrusage(self):
        usage = MODULE.read_usage(os.getpid())
        expected = resource.getrusage(resource.RUSAGE_SELF)
        seconds = (usage["user_time"] + usage["system_time"]) / 1e9
        self.assertAlmostEqual(seconds, expected.ru_utime + expected.ru_stime, delta=0.02)

    def test_converts_non_nanosecond_mach_ticks(self):
        self.assertEqual(MODULE.cpu_nanoseconds(24, 125, 3), 1000)
        self.assertEqual(MODULE.cpu_nanoseconds(123, 1, 1), 123)

    def samples(self):
        return [dict(
            elapsed_seconds=t, proc_start_abstime=42,
            user_time=int((100 + t / 10) * 1e9),
            system_time=int((50 + t / 20) * 1e9),
            pkg_idle_wakeups=300 + int(t * 2), interrupt_wakeups=600 + int(t * 3),
            resident_size=1000 + int(t * 10), phys_footprint=2000 + int(t * 20),
        ) for t in (0, 5, 10)]

    def test_uses_counter_deltas_and_preserves_memory_units(self):
        report = MODULE.summarize(self.samples())
        self.assertEqual(report["average_cpu_percent"], 15)
        self.assertEqual(report["pkg_idle_wakeups_per_second"], 2)
        self.assertEqual(report["interrupt_wakeups_per_second"], 3)
        self.assertEqual(report["average_resident_size_bytes"], 1050)
        self.assertEqual(report["peak_phys_footprint_bytes"], 2200)

    def test_refuses_reused_pid(self):
        samples = self.samples()
        samples[-1]["proc_start_abstime"] = 43
        with self.assertRaisesRegex(ValueError, "PID was reused"):
            MODULE.summarize(samples)

    def test_refuses_zero_length_interval(self):
        with self.assertRaisesRegex(ValueError, "positive elapsed"):
            MODULE.summarize(self.samples()[:1])

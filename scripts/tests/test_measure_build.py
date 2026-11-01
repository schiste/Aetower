import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "measure-build.py"
spec = importlib.util.spec_from_file_location("measure_build", SCRIPT)
measure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measure)


class MeasurementTests(unittest.TestCase):
    def test_aggregate_includes_grandchildren_and_excludes_unrelated_processes(self):
        processes = subprocess.CompletedProcess([], 0, "30 20 7\n10 1 3\n20 10 5\n40 1 100\n")
        with patch.object(measure.subprocess, "run", return_value=processes):
            self.assertEqual(measure.tree_rss(10), 15 * 1024)

    def test_unavailable_process_inspection_is_reported_as_missing(self):
        with patch.object(measure.subprocess, "run", side_effect=PermissionError):
            self.assertIsNone(measure.tree_rss(10))

    def test_failing_command_still_writes_report_and_preserves_exit_status(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "measurement.json"
            result = subprocess.run([sys.executable, str(SCRIPT), "--output", str(output), "--", sys.executable, "-c", "raise SystemExit(7)"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 7)
            report = json.loads(output.read_text())
            self.assertEqual(report["exit_status"], 7)
            self.assertGreater(report["wall_seconds"], 0)


if __name__ == "__main__":
    unittest.main()

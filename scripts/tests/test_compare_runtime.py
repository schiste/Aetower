"""Ownership and failure-path checks for retained-app comparisons."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch


spec = importlib.util.spec_from_file_location("compare_runtime", Path(__file__).resolve().parents[1] / "compare-runtime.py")
compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compare)


class RuntimeComparisonTests(unittest.TestCase):
    def test_requires_verified_absence_of_an_existing_app(self):
        for status in (0, 1, 2):
            with self.subTest(status=status), patch.object(compare.subprocess, "run", return_value=Mock(returncode=status)):
                if status == 1:
                    compare.require_no_running_app()
                else:
                    with self.assertRaises(RuntimeError):
                        compare.require_no_running_app()

    def test_sampler_failure_stops_only_the_owned_child_and_saves_status(self):
        process = Mock()
        process.poll.return_value = None
        process.pid = 123
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            report = {"status": "running", "windows": []}
            args = argparse.Namespace(settle=1, duration=1, interval=1)
            with patch.object(compare, "require_no_running_app"), patch.object(compare, "process_count", return_value=99), patch.object(compare.time, "sleep"), patch.object(compare.subprocess, "Popen", return_value=process), patch.object(compare.subprocess, "run", side_effect=subprocess.CalledProcessError(1, ["sampler"])):
                with self.assertRaises(subprocess.CalledProcessError):
                    compare.run_window({"app": directory, "variant": "baseline"}, "A1", output, args, report)
            process.terminate.assert_called_once_with()
            process.kill.assert_not_called()
            saved = json.loads((output / "summary.json").read_text())
            self.assertEqual(saved["windows"][0]["status"], "failed")

    def test_stop_waits_and_escalates_only_for_its_owned_process(self):
        process = Mock()
        process.poll.return_value = None
        process.wait.side_effect = [subprocess.TimeoutExpired("app", 15), 0]
        compare.stop_owned_app(process)
        process.terminate.assert_called_once_with()
        process.kill.assert_called_once_with()
        self.assertEqual(process.wait.call_count, 2)
        process.reset_mock()
        process.poll.return_value = 0
        compare.stop_owned_app(process)
        process.terminate.assert_not_called()
        process.kill.assert_not_called()


if __name__ == "__main__":
    unittest.main()

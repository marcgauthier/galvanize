#!/usr/bin/env python3

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


MODULE_PATH = Path(__file__).with_name("runner.py")
SPEC = importlib.util.spec_from_file_location("galvanize_long_running_runner", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = runner
SPEC.loader.exec_module(runner)


class AvailabilityScheduleTests(unittest.TestCase):
    def test_default_windows_have_requested_wall_clock_ratios(self):
        schedule = runner.AvailabilitySchedule(300)
        node4_offline = sum(
            schedule.should_be_offline(4, second + 0.5) for second in range(300)
        )
        node5_offline = sum(
            schedule.should_be_offline(5, second + 0.5) for second in range(300)
        )
        self.assertEqual(node4_offline, 99)
        self.assertEqual(node5_offline, 165)
        self.assertAlmostEqual(node4_offline / 330, 0.30)
        self.assertAlmostEqual(node5_offline / 330, 0.50)

    def test_window_boundaries_are_stable_when_accelerated(self):
        schedule = runner.AvailabilitySchedule(30)
        self.assertFalse(schedule.should_be_offline(4, 5.9))
        self.assertTrue(schedule.should_be_offline(4, 6.0))
        self.assertFalse(schedule.should_be_offline(4, 9.3))
        self.assertTrue(schedule.should_be_offline(5, 2.0))


class WorkloadModelTests(unittest.TestCase):
    def test_burst_cannot_skip_delete_thresholds(self):
        model = runner.WorkloadModel(1024)
        for _ in range(220):
            record_id, _payload, digest = model.make_insert(1)
            model.record_insert(record_id, 1, digest)
        deletions = model.deletions_due()
        self.assertEqual(len(deletions), 2)
        self.assertEqual(model.next_delete_at, 300)
        self.assertEqual(len(model.rows), 218)

    def test_payload_is_exact_size_and_digest_is_correct(self):
        model = runner.WorkloadModel(4096)
        record_id, payload, digest = model.make_insert(3)
        self.assertEqual(record_id, 1)
        self.assertEqual(len(payload), 4096)
        self.assertEqual(runner.hashlib.sha256(payload.encode()).hexdigest(), digest)

    def test_snapshot_matches_psql_line_format(self):
        model = runner.WorkloadModel(1024)
        for origin in (1, 2):
            record_id, _payload, digest = model.make_insert(origin)
            model.record_insert(record_id, origin, digest)
        expected = (
            f"1:1:1024:{model.rows[1][1]}\n"
            f"2:2:1024:{model.rows[2][1]}\n"
        ).encode()
        self.assertEqual(model.expected_snapshot(), expected)

    def test_samples_include_both_edges_and_are_deterministic(self):
        model = runner.WorkloadModel(1024)
        for _ in range(100):
            record_id, _payload, digest = model.make_insert(1)
            model.record_insert(record_id, 1, digest)
        first = model.sample_ids(4, 32)
        second = model.sample_ids(4, 32)
        self.assertEqual(first, second)
        self.assertEqual(len(first), 32)
        self.assertTrue({1, 2, 3, 4, 97, 98, 99, 100}.issubset(first))


class StorageAccountingTests(unittest.TestCase):
    def test_creates_storage_root_when_its_parent_exists(self):
        with tempfile.TemporaryDirectory() as temporary:
            storage = Path(temporary) / "fresh-storage"
            resolved = runner.ensure_storage_root(storage, {Path("/")})
            self.assertEqual(resolved, storage)
            self.assertTrue(storage.is_dir())

    def test_counts_database_files_and_excludes_logs(self):
        with tempfile.TemporaryDirectory() as temporary:
            node_dir = Path(temporary)
            (node_dir / "corrosion.db").write_bytes(b"x" * 8192)
            (node_dir / "logs").mkdir()
            (node_dir / "logs/agent.log").write_bytes(b"x" * 1024 * 1024)
            total = runner.node_database_bytes(node_dir)
            self.assertGreaterEqual(total, 8192)
            self.assertLess(total, 1024 * 1024)

    def test_counts_subscription_database_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            node_dir = Path(temporary)
            subscriptions = node_dir / "subscriptions"
            subscriptions.mkdir()
            (subscriptions / "one.db").write_bytes(b"x" * 8192)
            self.assertGreaterEqual(runner.node_database_bytes(node_dir), 8192)


class ProcessLifecycleTests(unittest.TestCase):
    def make_node(self, temporary: str):
        root = Path(temporary)
        settings = runner.Settings(
            root=root,
            test_dir=root,
            binary=Path("/bin/true"),
            runtime=root / "runtime",
            storage_root=root,
            capacity_bytes=1024,
            checkpoint_seconds=300,
            checkpoint_pause_seconds=30,
            payload_bytes=1024,
            operation_timeout_seconds=2,
            burst_interval_seconds=120,
            burst_quiet_seconds=20,
            burst_insert_count=20,
            sample_rows=32,
        )

        class QuietLog:
            def emit(self, _message):
                pass

        address = runner.NodeAddress.for_node(1)
        node = runner.Node(settings, address, "key", [address], QuietLog())
        node.node_dir.mkdir()
        (node.node_dir / "logs").mkdir()
        return node

    def test_unexpected_process_exit_is_reported(self):
        with tempfile.TemporaryDirectory() as temporary:
            node = self.make_node(temporary)
            node.process = subprocess.Popen(["/bin/false"], start_new_session=True)
            node.process.wait()
            node.expected_online = True
            with self.assertRaisesRegex(runner.TestFailure, "exited unexpectedly"):
                node.check_alive()

    def test_stop_reaps_process_and_clears_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            node = self.make_node(temporary)
            node.process = subprocess.Popen(["/bin/sleep", "30"], start_new_session=True)
            node.expected_online = True
            node.wait_for_ports_free = lambda: None
            node.stop(announce=False)
            self.assertIsNone(node.process)
            self.assertFalse(node.expected_online)

    def test_comparison_query_has_no_timeout(self):
        with tempfile.TemporaryDirectory() as temporary:
            node = self.make_node(temporary)
            node.process = SimpleNamespace(poll=lambda: None)
            node.expected_online = True
            completed = subprocess.CompletedProcess([], 0, stdout="", stderr="")
            with mock.patch.object(
                runner.subprocess, "run", return_value=completed
            ) as run:
                node.psql("SELECT 1;", unbounded=True)
                self.assertIsNone(run.call_args.kwargs["timeout"])

                node.psql("SELECT 1;")
                self.assertEqual(
                    run.call_args.kwargs["timeout"],
                    node.settings.operation_timeout_seconds,
                )


class CheckpointTests(unittest.TestCase):
    def test_capacity_stop_is_propagated_without_running_report(self):
        test = runner.LongRunningTest.__new__(runner.LongRunningTest)
        test.checkpoint_number = 0
        test.settings = SimpleNamespace(checkpoint_pause_seconds=30)
        test.nodes = []

        class QuietLog:
            def emit(self, _message):
                pass

        test.event_log = QuietLog()
        test.check_expected_processes = lambda: None
        test.check_capacity = lambda: True
        test.report = lambda _name: self.fail("report ran after capacity was reached")

        self.assertTrue(test.checkpoint())
        self.assertEqual(test.checkpoint_number, 1)


if __name__ == "__main__":
    unittest.main()

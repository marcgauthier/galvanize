#!/usr/bin/env python3
"""Reliable five-node, long-running Galvanize live-test orchestrator."""

from __future__ import annotations

import hashlib
import json
import os
import random
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path


SCENARIO = "long-running-five-node"
NODE_COUNT = 5


class TestFailure(RuntimeError):
    pass


def env_int(name: str, default: int, minimum: int = 1) -> int:
    raw = os.environ.get(name, str(default))
    try:
        value = int(raw)
    except ValueError as error:
        raise TestFailure(f"{name} must be an integer, got {raw!r}") from error
    if value < minimum:
        raise TestFailure(f"{name} must be at least {minimum}, got {value}")
    return value


@dataclass(frozen=True)
class Settings:
    root: Path
    test_dir: Path
    binary: Path
    runtime: Path
    storage_root: Path
    capacity_bytes: int
    checkpoint_seconds: int
    checkpoint_pause_seconds: int
    payload_bytes: int
    operation_timeout_seconds: int
    burst_interval_seconds: int
    burst_quiet_seconds: int
    burst_insert_count: int
    sample_rows: int

    @classmethod
    def from_environment(cls) -> "Settings":
        test_dir = Path(__file__).resolve().parent
        root = test_dir.parent.parent
        return cls(
            root=root,
            test_dir=test_dir,
            binary=Path(os.environ.get("GALVANIZE_BIN", root / "target/debug/corrosion")),
            runtime=Path(os.environ.get("GALVANIZE_LIVE_RUNTIME", test_dir / "runtime")),
            storage_root=Path(
                os.environ.get(
                    "GALVANIZE_LIVE_LONG_RUNNING_STORAGE_ROOT",
                    "/media/marc/2TB/DATA_GALVANIZE",
                )
            ),
            capacity_bytes=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_CAPACITY_BYTES", 25 * 1024**3
            ),
            checkpoint_seconds=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_CHECKPOINT_SECONDS", 300
            ),
            checkpoint_pause_seconds=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_CHECKPOINT_PAUSE_SECONDS", 30
            ),
            payload_bytes=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_PAYLOAD_BYTES", 256 * 1024, 1024
            ),
            operation_timeout_seconds=env_int(
                "GALVANIZE_LIVE_TIMEOUT_SECONDS", 60
            ),
            burst_interval_seconds=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_BURST_INTERVAL_SECONDS", 120
            ),
            burst_quiet_seconds=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_BURST_QUIET_SECONDS", 20
            ),
            burst_insert_count=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_BURST_INSERT_COUNT", 20
            ),
            sample_rows=env_int(
                "GALVANIZE_LIVE_LONG_RUNNING_SAMPLE_ROWS", 32
            ),
        )


@dataclass(frozen=True)
class NodeAddress:
    number: int
    api_host: str
    api_port: int
    pg_host: str
    pg_port: int
    gossip_host: str
    gossip_port: int

    @classmethod
    def for_node(cls, number: int) -> "NodeAddress":
        return cls(
            number=number,
            api_host="127.0.0.1",
            api_port=44200 + number,
            pg_host="127.0.0.1",
            pg_port=55200 + number,
            gossip_host=f"127.0.0.{200 + number}",
            gossip_port=48200 + number,
        )


class EventLog:
    def __init__(self, path: Path):
        self.path = path
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.file = self.path.open("a", encoding="utf-8", buffering=1)

    def emit(self, message: str) -> None:
        print(message, flush=True)
        if not self.file.closed:
            self.file.write(f"{datetime.now(timezone.utc).isoformat()} {message}\n")

    def close(self) -> None:
        if not self.file.closed:
            self.file.close()


def tcp_port_is_free(host: str, port: int) -> bool:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.bind((host, port))
        return True
    except OSError:
        return False
    finally:
        sock.close()


def udp_port_is_free(host: str, port: int) -> bool:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.bind((host, port))
        return True
    except OSError:
        return False
    finally:
        sock.close()


def allocated_file_bytes(path: Path) -> int:
    stat = path.stat()
    return stat.st_blocks * 512


def node_database_bytes(node_dir: Path) -> int:
    total = 0
    for name in ("corrosion.db", "corrosion.db-wal", "corrosion.db-shm"):
        path = node_dir / name
        if path.is_file():
            total += allocated_file_bytes(path)
    subscriptions = node_dir / "subscriptions"
    if subscriptions.is_dir():
        total += sum(
            allocated_file_bytes(path)
            for path in subscriptions.rglob("*")
            if path.is_file()
        )
    return total


def ensure_storage_root(path: Path, forbidden: set[Path]) -> Path:
    requested = path.expanduser()
    if not requested.is_absolute():
        requested = Path.cwd() / requested
    if not requested.exists():
        try:
            parent = requested.parent.resolve(strict=True)
        except OSError as error:
            raise TestFailure(
                f"storage root parent does not exist: {requested.parent}"
            ) from error
        requested = parent / requested.name
        try:
            requested.mkdir(mode=0o700)
        except OSError as error:
            raise TestFailure(
                f"could not create storage root: {requested}: {error}"
            ) from error
    try:
        storage = requested.resolve(strict=True)
    except OSError as error:
        raise TestFailure(f"could not resolve storage root: {requested}: {error}") from error
    if not storage.is_dir():
        raise TestFailure(f"storage root is not a directory: {storage}")
    if storage in forbidden:
        raise TestFailure(f"refusing unsafe storage root: {storage}")
    return storage


class AvailabilitySchedule:
    """Offline windows scaled to the active (non-checkpoint) period."""

    NODE_4 = ((0.20, 0.31), (8 / 15, 193 / 300), (13 / 15, 293 / 300))
    NODE_5 = ((1 / 15, 0.25), (0.40, 7 / 12), (11 / 15, 11 / 12))

    def __init__(self, active_seconds: int):
        self.active_seconds = active_seconds

    def should_be_offline(self, node_number: int, elapsed: float) -> bool:
        windows = self.NODE_4 if node_number == 4 else self.NODE_5
        fraction = elapsed / self.active_seconds
        return any(start <= fraction < end for start, end in windows)


class WorkloadModel:
    def __init__(self, payload_bytes: int):
        self.payload_bytes = payload_bytes
        self.next_id = 1
        self.insert_count = 0
        self.next_delete_at = 100
        self.rows: dict[int, tuple[int, str, int]] = {}
        self.rng = random.Random(0x47414C56)

    def make_insert(self, origin: int) -> tuple[int, str, str]:
        record_id = self.next_id
        prefix = f"node:{origin}:record:{record_id}:"
        if len(prefix) > self.payload_bytes:
            raise TestFailure("configured payload is too small for its deterministic prefix")
        payload = prefix + "x" * (self.payload_bytes - len(prefix))
        digest = hashlib.sha256(payload.encode()).hexdigest()
        return record_id, payload, digest

    def record_insert(self, record_id: int, origin: int, digest: str) -> None:
        self.rows[record_id] = (origin, digest, self.payload_bytes)
        self.next_id += 1
        self.insert_count += 1

    def deletions_due(self) -> list[int]:
        deletions: list[int] = []
        while self.insert_count >= self.next_delete_at and self.rows:
            record_id = self.rng.choice(tuple(self.rows))
            deletions.append(record_id)
            del self.rows[record_id]
            self.next_delete_at += 100
        return deletions

    def expected_snapshot(self) -> bytes:
        return "".join(
            f"{record_id}:{origin}:{length}:{digest}\n"
            for record_id, (origin, digest, length) in sorted(self.rows.items())
        ).encode()

    def sample_ids(self, checkpoint_number: int, limit: int) -> list[int]:
        ids = sorted(self.rows)
        if len(ids) <= limit:
            return ids
        edge = min(4, limit // 4)
        selected = set(ids[:edge] + ids[-edge:])
        rng = random.Random(0xC0FFEE + checkpoint_number)
        remaining = [record_id for record_id in ids if record_id not in selected]
        selected.update(rng.sample(remaining, limit - len(selected)))
        return sorted(selected)


class Node:
    def __init__(
        self,
        settings: Settings,
        address: NodeAddress,
        key: str,
        all_addresses: list[NodeAddress],
        event_log: EventLog,
    ):
        self.settings = settings
        self.address = address
        self.key = key
        self.all_addresses = all_addresses
        self.event_log = event_log
        self.node_dir = settings.storage_root / f"node{address.number}"
        self.log_path = self.node_dir / "logs/agent.log"
        self.process: subprocess.Popen[bytes] | None = None
        self.log_file = None
        self.expected_online = False
        self.offline_started: float | None = None
        self.offline_seconds = 0.0

    @property
    def number(self) -> int:
        return self.address.number

    @property
    def pg_url(self) -> str:
        return (
            f"postgresql://postgres@{self.address.pg_host}:"
            f"{self.address.pg_port}/postgres"
        )

    def prepare(self) -> None:
        schema_dir = self.node_dir / "schema"
        log_dir = self.node_dir / "logs"
        schema_dir.mkdir(parents=True, exist_ok=True)
        log_dir.mkdir(parents=True, exist_ok=True)
        (schema_dir / "long-running.sql").write_text(
            """CREATE TABLE IF NOT EXISTS long_running_records (
  id INTEGER PRIMARY KEY NOT NULL,
  origin INTEGER NOT NULL DEFAULT 0,
  payload TEXT NOT NULL DEFAULT '',
  payload_sha256 TEXT NOT NULL DEFAULT ''
) WITHOUT ROWID;
""",
            encoding="utf-8",
        )
        bootstrap = [
            f'"{peer.gossip_host}:{peer.gossip_port}"'
            for peer in self.all_addresses
            if peer.number != self.number
        ]
        config = f"""[db]
path = "{self.node_dir / 'corrosion.db'}"
schema_paths = ["{schema_dir}"]
await-unlock = true
[api]
addr = "{self.address.api_host}:{self.address.api_port}"
[[api.pg]]
addr = "{self.address.pg_host}:{self.address.pg_port}"
[gossip]
addr = "{self.address.gossip_host}:{self.address.gossip_port}"
client_addr_v4 = "{self.address.gossip_host}:0"
bootstrap = [{', '.join(bootstrap)}]
plaintext = true
allow-list = ["*"]
[admin]
path = "{self.node_dir / 'admin.sock'}"
[perf]
min_sync_backoff = 1
max_sync_backoff = 2
[log]
format = "json"
"""
        (self.node_dir / "config.toml").write_text(config, encoding="utf-8")

    def ports_free(self) -> bool:
        return (
            tcp_port_is_free(self.address.api_host, self.address.api_port)
            and tcp_port_is_free(self.address.pg_host, self.address.pg_port)
            and udp_port_is_free(self.address.gossip_host, self.address.gossip_port)
        )

    def wait_for_ports_free(self) -> None:
        deadline = time.monotonic() + self.settings.operation_timeout_seconds
        while time.monotonic() < deadline:
            if self.ports_free():
                return
            time.sleep(0.1)
        raise TestFailure(f"node{self.number} listeners were not released after stop")

    def log_tail(self, max_lines: int = 30, max_chars: int = 6000) -> str:
        if not self.log_path.exists():
            return "(agent log does not exist)"
        lines = self.log_path.read_text(encoding="utf-8", errors="replace").splitlines()
        text = "\n".join(line[:2000] for line in lines[-max_lines:])
        return text[-max_chars:]

    def check_alive(self) -> None:
        if not self.expected_online:
            return
        if self.process is None:
            raise TestFailure(f"node{self.number} is marked online without a process")
        code = self.process.poll()
        if code is not None:
            raise TestFailure(
                f"node{self.number} exited unexpectedly with status {code}\n"
                f"--- node{self.number} log tail ---\n{self.log_tail()}"
            )

    def start(self) -> None:
        if self.expected_online:
            self.check_alive()
            return
        self.wait_for_ports_free()
        self.event_log.emit(
            f"  START     node{self.number} pg={self.address.pg_host}:"
            f"{self.address.pg_port} gossip={self.address.gossip_host}:"
            f"{self.address.gossip_port}"
        )
        self.log_file = self.log_path.open("ab", buffering=0)
        self.log_file.write(
            f"\n--- restart {datetime.now(timezone.utc).isoformat()} ---\n".encode()
        )
        environment = os.environ.copy()
        environment["RUST_LOG"] = os.environ.get("GALVANIZE_LIVE_RUST_LOG", "warn")
        try:
            self.process = subprocess.Popen(
                [
                    str(self.settings.binary),
                    "--config",
                    str(self.node_dir / "config.toml"),
                    "agent",
                ],
                stdout=self.log_file,
                stderr=subprocess.STDOUT,
                env=environment,
                start_new_session=True,
            )
        except Exception:
            self.log_file.close()
            self.log_file = None
            raise
        self.expected_online = True
        try:
            self._unlock()
            self._wait_ready()
        except Exception:
            self.stop(announce=False)
            raise
        if self.offline_started is not None:
            self.offline_seconds += time.monotonic() - self.offline_started
            self.offline_started = None
        self.event_log.emit(
            f"  READY     node{self.number} pg={self.address.pg_host}:"
            f"{self.address.pg_port} schema=long_running_records"
        )

    def _unlock(self) -> None:
        deadline = time.monotonic() + self.settings.operation_timeout_seconds
        body = json.dumps({"key": self.key, "cipher": "chacha20"}).encode()
        url = (
            f"http://{self.address.api_host}:{self.address.api_port}"
            "/v1/admin/unlock"
        )
        last_error = "endpoint did not respond"
        while time.monotonic() < deadline:
            self.check_alive()
            request = urllib.request.Request(
                url, data=body, headers={"Content-Type": "application/json"}, method="POST"
            )
            try:
                with urllib.request.urlopen(request, timeout=1) as response:
                    if 200 <= response.status < 300:
                        return
            except urllib.error.HTTPError as error:
                if error.code in (400, 401, 403):
                    raise TestFailure(
                        f"node{self.number} rejected its HTTP unlock key: HTTP {error.code}"
                    ) from error
                last_error = f"HTTP {error.code}"
            except (urllib.error.URLError, TimeoutError, ConnectionError) as error:
                last_error = str(error)
            time.sleep(0.1)
        raise TestFailure(
            f"node{self.number} unlock timed out: {last_error}\n"
            f"--- node{self.number} log tail ---\n{self.log_tail()}"
        )

    def _wait_ready(self) -> None:
        deadline = time.monotonic() + self.settings.operation_timeout_seconds
        last_error = "PostgreSQL listener did not respond"
        while time.monotonic() < deadline:
            self.check_alive()
            try:
                self.psql("SELECT count(*) FROM long_running_records;", timeout=3)
                return
            except TestFailure as error:
                last_error = str(error).splitlines()[0]
            time.sleep(0.2)
        raise TestFailure(
            f"node{self.number} readiness timed out: {last_error}\n"
            f"--- node{self.number} log tail ---\n{self.log_tail()}"
        )

    def stop(self, announce: bool = True) -> None:
        if self.process is None:
            self.expected_online = False
            return
        if announce:
            self.event_log.emit(f"  OFFLINE   node{self.number}")
        self.expected_online = False
        if self.process.poll() is None:
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            self.process.wait(timeout=self.settings.operation_timeout_seconds)
        except subprocess.TimeoutExpired as error:
            raise TestFailure(f"node{self.number} process could not be reaped") from error
        self.process = None
        if self.log_file is not None:
            self.log_file.close()
            self.log_file = None
        if self.offline_started is None:
            self.offline_started = time.monotonic()
        self.wait_for_ports_free()

    def psql(
        self,
        sql: str,
        timeout: int | None = None,
        fields: bool = False,
        unbounded: bool = False,
    ) -> str:
        self.check_alive()
        command = ["psql", self.pg_url, "-X", "-v", "ON_ERROR_STOP=1", "-qAt"]
        if fields:
            command.extend(["-F", "\t"])
        effective_timeout = (
            None if unbounded else timeout or self.settings.operation_timeout_seconds
        )
        try:
            result = subprocess.run(
                command,
                input=sql,
                text=True,
                capture_output=True,
                timeout=effective_timeout,
                check=False,
            )
        except subprocess.TimeoutExpired as error:
            raise TestFailure(f"node{self.number} PostgreSQL operation timed out") from error
        if result.returncode:
            self.check_alive()
            detail = result.stderr.strip()[-2000:]
            raise TestFailure(
                f"node{self.number} PostgreSQL operation failed with status "
                f"{result.returncode}: {detail}"
            )
        return result.stdout


class LongRunningTest:
    def __init__(self, settings: Settings, event_log: EventLog):
        self.settings = settings
        self.event_log = event_log
        self.addresses = [NodeAddress.for_node(number) for number in range(1, 6)]
        self.keys = self._load_keys()
        self.nodes = [
            Node(settings, address, self.keys[address.number], self.addresses, event_log)
            for address in self.addresses
        ]
        self.schedule = AvailabilitySchedule(settings.checkpoint_seconds)
        self.model = WorkloadModel(settings.payload_bytes)
        self.checkpoint_number = 0
        self.started_at = time.monotonic()

    def _load_keys(self) -> dict[int, str]:
        keys: dict[int, str] = {}
        for number in range(1, NODE_COUNT + 1):
            name = f"GALVANIZE_LIVE_LONG_RUNNING_DB_KEY_NODE{number}"
            value = os.environ.get(name, "")
            if not value:
                raise TestFailure(f"missing required environment variable: {name}")
            keys[number] = value
        if len(set(keys.values())) != NODE_COUNT:
            raise TestFailure("node encryption keys must be unique")
        return keys

    def preflight(self) -> None:
        if not self.settings.binary.is_file() or not os.access(self.settings.binary, os.X_OK):
            raise TestFailure("build first: cargo build -p corrosion")
        if shutil.which("psql") is None:
            raise TestFailure("psql is required for live tests")
        forbidden = {Path("/").resolve(), self.settings.root.resolve()}
        storage = ensure_storage_root(self.settings.storage_root, forbidden)
        for number in range(1, NODE_COUNT + 1):
            candidate = storage / f"node{number}"
            if candidate.exists() and candidate.resolve() != candidate:
                raise TestFailure(f"refusing symlinked node path: {candidate}")
        probe = storage / f".galvanize-live-write-probe-{os.getpid()}"
        try:
            probe.write_bytes(b"")
            probe.unlink()
        except OSError as error:
            raise TestFailure(f"storage root is not writable: {storage}: {error}") from error
        busy = [node.number for node in self.nodes if not node.ports_free()]
        if busy:
            raise TestFailure(f"listener ports are already in use for nodes: {busy}")

    def reset_storage(self) -> None:
        storage = self.settings.storage_root.resolve(strict=True)
        for number in range(1, NODE_COUNT + 1):
            node_dir = storage / f"node{number}"
            if node_dir.exists():
                shutil.rmtree(node_dir)
            node_dir.mkdir()
        for node in self.nodes:
            node.prepare()

    def total_database_bytes(self) -> int:
        return sum(node_database_bytes(node.node_dir) for node in self.nodes)

    def check_capacity(self) -> bool:
        total = self.total_database_bytes()
        if total < self.settings.capacity_bytes:
            return False
        self.event_log.emit(
            f"  CAPACITY  reached total_db_bytes={total} "
            f"threshold={self.settings.capacity_bytes}; stopping immediately"
        )
        for node in self.nodes:
            self.event_log.emit(
                f"  STORAGE   node{node.number} db_bytes={node_database_bytes(node.node_dir)}"
            )
        return True

    def check_expected_processes(self) -> None:
        for node in self.nodes:
            node.check_alive()

    def ensure_availability(self, elapsed: float) -> None:
        for number in (4, 5):
            node = self.nodes[number - 1]
            should_be_offline = self.schedule.should_be_offline(number, elapsed)
            if should_be_offline and node.expected_online:
                node.stop()
            elif not should_be_offline and not node.expected_online:
                node.start()

    def pick_online_node(self, preferred: int) -> Node:
        for offset in range(NODE_COUNT):
            number = ((preferred - 1 + offset) % NODE_COUNT) + 1
            node = self.nodes[number - 1]
            if node.expected_online:
                node.check_alive()
                return node
        raise TestFailure("no online node is available for the workload")

    def insert_one(self) -> None:
        preferred = ((self.model.next_id - 1) % NODE_COUNT) + 1
        node = self.pick_online_node(preferred)
        record_id, payload, digest = self.model.make_insert(node.number)
        sql = (
            "INSERT INTO long_running_records "
            "(id, origin, payload, payload_sha256) VALUES "
            f"({record_id}, {node.number}, '{payload}', '{digest}');\n"
        )
        node.psql(sql)
        self.model.record_insert(record_id, node.number, digest)
        for delete_id in self.model.deletions_due():
            delete_node = self.pick_online_node(delete_id)
            delete_node.psql(
                f"DELETE FROM long_running_records WHERE id = {delete_id};\n"
            )
            self.event_log.emit(
                f"  DELETE    id={delete_id} after {self.model.insert_count} inserts"
            )

    def sleep_managed(self, seconds: float, cycle_start: float) -> bool:
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self.check_expected_processes()
            self.ensure_availability(time.monotonic() - cycle_start)
            if self.check_capacity():
                return True
            time.sleep(min(0.25, max(0.0, deadline - time.monotonic())))
        return False

    def checkpoint(self) -> bool:
        self.checkpoint_number += 1
        self.event_log.emit(
            f"  PAUSE     checkpoint={self.checkpoint_number}: forcing all nodes "
            f"online for {self.settings.checkpoint_pause_seconds}s"
        )
        for node in self.nodes:
            if not node.expected_online:
                node.start()
        settle_deadline = time.monotonic() + self.settings.checkpoint_pause_seconds
        while time.monotonic() < settle_deadline:
            self.check_expected_processes()
            if self.check_capacity():
                return True
            time.sleep(min(0.25, settle_deadline - time.monotonic()))
        self.report(f"CHECKPOINT {self.checkpoint_number}")
        return False

    def report(self, name: str) -> None:
        total = self.total_database_bytes()
        expected_count = len(self.model.rows)
        expected_hash = hashlib.sha256(self.model.expected_snapshot()).hexdigest()
        sample_ids = self.model.sample_ids(
            self.checkpoint_number, self.settings.sample_rows
        )
        failures: list[str] = []
        self.event_log.emit(
            f"  REPORT    {name} total_db_bytes={total} "
            f"capacity_bytes={self.settings.capacity_bytes} expected_rows={expected_count}"
        )
        id_list = ",".join(str(record_id) for record_id in sample_ids)
        for node in self.nodes:
            node.check_alive()
            comparison_started = time.monotonic()
            self.event_log.emit(
                f"  COMPARE   node{node.number} scan started timeout=none"
            )
            rows_text = node.psql(
                "SELECT count(*) FROM long_running_records;\n", unbounded=True
            ).strip()
            try:
                rows = int(rows_text)
            except ValueError:
                failures.append(f"node{node.number} returned invalid row count {rows_text!r}")
                rows = -1
            snapshot = node.psql(
                "SELECT id || ':' || origin || ':' || length(payload) || ':' || "
                "payload_sha256 FROM long_running_records ORDER BY id;\n",
                unbounded=True,
            ).encode()
            digest = hashlib.sha256(snapshot).hexdigest()
            if rows != expected_count:
                failures.append(
                    f"node{node.number} rows={rows}, expected={expected_count}"
                )
            if digest != expected_hash:
                failures.append(
                    f"node{node.number} metadata sha256={digest}, expected={expected_hash}"
                )
            if sample_ids:
                sample_output = node.psql(
                    "SELECT id, payload, payload_sha256 FROM long_running_records "
                    f"WHERE id IN ({id_list}) ORDER BY id;\n",
                    fields=True,
                    unbounded=True,
                )
                seen: set[int] = set()
                for line in sample_output.splitlines():
                    fields = line.split("\t", 2)
                    if len(fields) != 3:
                        failures.append(f"node{node.number} returned malformed sample row")
                        continue
                    record_id = int(fields[0])
                    payload = fields[1]
                    stored_digest = fields[2]
                    actual_digest = hashlib.sha256(payload.encode()).hexdigest()
                    expected = self.model.rows.get(record_id)
                    if expected is None or actual_digest != stored_digest or actual_digest != expected[1]:
                        failures.append(
                            f"node{node.number} payload integrity failed for id={record_id}"
                        )
                    seen.add(record_id)
                missing = set(sample_ids) - seen
                if missing:
                    failures.append(
                        f"node{node.number} omitted sampled ids={sorted(missing)}"
                    )
            db_bytes = node_database_bytes(node.node_dir)
            self.event_log.emit(
                f"  COMPARE   node{node.number} rows={rows} sha256={digest} "
                f"db_bytes={db_bytes} "
                f"elapsed_seconds={time.monotonic() - comparison_started:.1f}"
            )
        elapsed = max(1.0, time.monotonic() - self.started_at)
        for number in (4, 5):
            node = self.nodes[number - 1]
            downtime = node.offline_seconds
            self.event_log.emit(
                f"  AVAIL     node{number} offline_seconds={downtime:.1f} "
                f"offline_percent={downtime * 100 / elapsed:.1f}"
            )
        if failures:
            raise TestFailure("checkpoint mismatch:\n  " + "\n  ".join(failures))
        self.event_log.emit(
            f"  CHECK     all five nodes synchronized; sampled_payloads={len(sample_ids)}"
        )

    def shutdown(self) -> list[str]:
        errors: list[str] = []
        for node in self.nodes:
            if node.process is not None:
                try:
                    node.stop(announce=False)
                except Exception as error:  # cleanup must continue for every node
                    errors.append(str(error))
        if errors:
            self.event_log.emit("  CLEANUP   " + "; ".join(errors))
        return errors

    def run(self) -> str:
        self.preflight()
        self.reset_storage()
        self.event_log.emit(f"== {SCENARIO}: persistent encrypted five-node mesh ==")
        self.event_log.emit(
            f"  STORAGE   {self.settings.storage_root}/node{{1,2,3,4,5}} "
            "(resetting all five directories)"
        )
        self.event_log.emit(
            f"  PLAN      ~1 insert/second, {self.settings.payload_bytes} byte rows, "
            f"{self.settings.burst_quiet_seconds}s quiet then "
            f"{self.settings.burst_insert_count}-row bursts"
        )
        self.event_log.emit(
            f"  CAPACITY  stop at {self.settings.capacity_bytes} allocated DB bytes"
        )
        for node in self.nodes:
            node.start()
        self.report("INITIAL")
        cycle_start = time.monotonic()
        next_burst = cycle_start + self.settings.burst_interval_seconds
        while True:
            self.check_expected_processes()
            if self.check_capacity():
                return "CAPACITY_REACHED"
            elapsed = time.monotonic() - cycle_start
            if elapsed >= self.settings.checkpoint_seconds:
                if self.checkpoint():
                    return "CAPACITY_REACHED"
                cycle_start = time.monotonic()
                next_burst = cycle_start + self.settings.burst_interval_seconds
                continue
            self.ensure_availability(elapsed)
            if time.monotonic() >= next_burst:
                self.event_log.emit(
                    f"  BURST     pausing {self.settings.burst_quiet_seconds}s, then "
                    f"committing {self.settings.burst_insert_count} inserts"
                )
                if self.sleep_managed(self.settings.burst_quiet_seconds, cycle_start):
                    return "CAPACITY_REACHED"
                for _ in range(self.settings.burst_insert_count):
                    self.ensure_availability(time.monotonic() - cycle_start)
                    if self.check_capacity():
                        return "CAPACITY_REACHED"
                    self.insert_one()
                next_burst += self.settings.burst_interval_seconds
                if next_burst <= time.monotonic():
                    next_burst = time.monotonic() + self.settings.burst_interval_seconds
            else:
                tick_deadline = time.monotonic() + 1.0
                self.insert_one()
                remaining = tick_deadline - time.monotonic()
                if remaining > 0 and self.sleep_managed(remaining, cycle_start):
                    return "CAPACITY_REACHED"


def preserve_failure_runtime(settings: Settings) -> Path | None:
    if not settings.runtime.exists():
        return None
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    destination = settings.root / "tests-live/failures" / f"{stamp}-{SCENARIO}"
    suffix = 1
    while destination.exists():
        destination = destination.with_name(f"{stamp}-{SCENARIO}-{suffix}")
        suffix += 1
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(settings.runtime), destination)
    return destination


def main() -> int:
    settings: Settings | None = None
    event_log: EventLog | None = None
    test: LongRunningTest | None = None
    started = time.monotonic()
    def interrupt(_signum, _frame):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)
    try:
        settings = Settings.from_environment()
        if settings.runtime.exists():
            shutil.rmtree(settings.runtime)
        settings.runtime.mkdir(parents=True)
        event_log = EventLog(settings.runtime / "events.log")
        test = LongRunningTest(settings, event_log)
        result = test.run()
        cleanup_errors = test.shutdown()
        if cleanup_errors:
            raise TestFailure("cleanup failed: " + "; ".join(cleanup_errors))
        event_log.emit(
            f"RESULT: {result} scenario={SCENARIO} elapsed={time.monotonic() - started:.1f}s"
        )
        event_log.close()
        shutil.rmtree(settings.runtime)
        return 0
    except KeyboardInterrupt:
        if test is not None:
            test.shutdown()
        if event_log is not None:
            event_log.emit(
                f"RESULT: INTERRUPTED scenario={SCENARIO} elapsed={time.monotonic() - started:.1f}s"
            )
            event_log.close()
        return 130
    except Exception as error:
        if test is not None:
            test.shutdown()
        message = f"RESULT: FAIL scenario={SCENARIO}: {error}"
        if event_log is not None:
            event_log.emit(message)
            event_log.close()
        else:
            print(message, file=sys.stderr, flush=True)
        if settings is not None:
            failure_dir = preserve_failure_runtime(settings)
            if failure_dir is not None:
                print(f"failure artifacts retained at {failure_dir}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

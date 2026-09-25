"""HTTP(S), event-stream, and mTLS control APIs for GALVANIZE."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, BinaryIO, Iterator, Mapping
from urllib.parse import quote, urlsplit, urlunsplit

import requests


class APIError(requests.HTTPError):
    """Non-success API response with status and response body retained."""


class AdminCommandError(RuntimeError):
    def __init__(self, message: str, result: dict):
        super().__init__(f"GALVANIZE admin command failed: {message}")
        self.result = result


def _base_url(value: str | None, required: bool = False) -> str:
    if not value:
        if required:
            raise ValueError("API URL is required")
        return ""
    parsed = urlsplit(value)
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise ValueError("expected an http or https URL with a host")
    return urlunsplit((parsed.scheme, parsed.netloc, parsed.path.rstrip("/"), "", ""))


@dataclass(frozen=True)
class Statement:
    query: str
    params: list[Any] | None = None
    named_params: Mapping[str, Any] | None = None

    def wire(self):
        if not self.query:
            raise ValueError("SQL query must not be empty")
        if self.named_params is not None:
            return {"query": self.query, "named_params": _json_value(self.named_params)}
        if self.params is not None:
            return [self.query, _json_value(self.params)]
        return self.query


def _json_value(value):
    if isinstance(value, (bytes, bytearray, memoryview)):
        return list(bytes(value))
    if isinstance(value, Mapping):
        return {k: _json_value(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_value(v) for v in value]
    return value


class EventStream(Iterator[dict]):
    """Line-delimited JSON stream. Use as a context manager to ensure closure."""

    def __init__(self, response: requests.Response):
        self.response = response
        self.query_id = response.headers.get("corro-query-id")
        self.query_hash = response.headers.get("corro-query-hash")
        self._lines = response.iter_lines(decode_unicode=True)

    def __iter__(self):
        return self

    def __next__(self):
        for line in self._lines:
            if line:
                if isinstance(line, bytes):
                    line = line.decode("utf-8")
                return json.loads(line)
        self.close()
        raise StopIteration

    def close(self):
        self.response.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


class Client:
    """GALVANIZE client. `verify` and `cert` configure CA and client mTLS."""

    def __init__(self, api_url: str, *, admin_url: str | None = None,
                 highlow_url: str | None = None, bearer_token: str | None = None,
                 verify: bool | str = True, cert: str | tuple[str, str] | None = None,
                 timeout: float | tuple[float, float] | None = None,
                 session: requests.Session | None = None):
        self.api_url = _base_url(api_url, required=True)
        self.admin_url = _base_url(admin_url)
        self.highlow_url = _base_url(highlow_url)
        self.token, self.verify, self.cert, self.timeout = bearer_token, verify, cert, timeout
        self.session = session or requests.Session()

    def _request(self, method: str, base: str, path: str, *, params=None, json_body=None,
                 data=None, headers=None, stream=False):
        if not base:
            raise RuntimeError("this API endpoint is not configured")
        request_headers = dict(headers or {})
        if base == self.api_url and self.token and path not in ("/v1/admin/commands",) and not path.startswith("/v1/highlow/"):
            request_headers["Authorization"] = f"Bearer {self.token}"
        response = self.session.request(
            method, base + path, params=params, json=json_body, data=data,
            headers=request_headers, verify=self.verify, cert=self.cert,
            timeout=self.timeout, stream=stream,
        )
        if not response.ok:
            error = APIError(f"GALVANIZE API returned HTTP {response.status_code}: {response.text}")
            error.response = response
            response.close()
            raise error
        return response

    def _json(self, method, base, path, **kwargs):
        response = self._request(method, base, path, **kwargs)
        try:
            if response.status_code == 204 or not response.content:
                return None
            return response.json()
        finally:
            response.close()

    def query(self, statement: Statement | str, *, timeout: int | None = None) -> EventStream:
        query = {"timeout": timeout} if timeout else None
        return EventStream(self._request("POST", self.api_url, "/v1/queries", params=query,
                                         json_body=_wire(statement), stream=True))

    def transaction(self, statements: list[Statement | str], *, timeout: int | None = None):
        query = {"timeout": timeout} if timeout else None
        return self._json("POST", self.api_url, "/v1/transactions", params=query,
                          json_body=[_wire(s) for s in statements])

    def subscribe(self, statement: Statement | str, *, from_change: int | None = None,
                  skip_rows: bool = False) -> EventStream:
        params = {"skip_rows": str(skip_rows).lower()}
        if from_change is not None:
            params["from"] = from_change
        return EventStream(self._request("POST", self.api_url, "/v1/subscriptions", params=params,
                                         json_body=_wire(statement), stream=True))

    def resume_subscription(self, subscription_id: str, *, from_change: int | None = None,
                            skip_rows: bool = False) -> EventStream:
        params = {"skip_rows": str(skip_rows).lower()}
        if from_change is not None:
            params["from"] = from_change
        path = "/v1/subscriptions/" + quote(subscription_id, safe="")
        return EventStream(self._request("GET", self.api_url, path, params=params, stream=True))

    def updates(self, table: str) -> EventStream:
        path = "/v1/updates/" + quote(table, safe="")
        return EventStream(self._request("POST", self.api_url, path, stream=True))

    def table_stats(self, tables: list[str]):
        return self._json("POST", self.api_url, "/v1/table_stats", json_body={"tables": tables})

    def health(self, **thresholds):
        return self._json("GET", self.api_url, "/v1/health", params=thresholds or None)

    def unlock_from_env(self, key_env: str = "GALVANIZE_DB_KEY", *, cipher=None, cipher_params=None):
        if not self.api_url.startswith("https://"):
            raise ValueError("database unlock requires an HTTPS API URL")
        try:
            key = os.environ[key_env]
        except KeyError as exc:
            raise RuntimeError(f"database key environment variable {key_env!r} is unset") from exc
        return self._json("POST", self.api_url, "/v1/admin/unlock",
                          json_body={"key": key, "cipher": cipher, "cipher_params": cipher_params})

    def upload_file(self, filename: str, contents: BinaryIO | bytes, *, content_type="application/octet-stream"):
        return self._json("POST", self.api_url, "/v1/files/upload", params={"filename": filename},
                          data=contents, headers={"Content-Type": content_type})

    def upload_file_with_uuid(self, uuid: str, filename: str, contents: BinaryIO | bytes,
                              *, content_type="application/octet-stream"):
        path = "/v1/files/" + quote(uuid, safe="")
        return self._json("POST", self.api_url, path,
                          params={"filename": filename, "content_type": content_type},
                          data=contents, headers={"Content-Type": content_type})

    def get_file(self, uuid: str) -> requests.Response:
        return self._request("GET", self.api_url, "/v1/files/" + quote(uuid, safe=""), stream=True)

    def peer_fetch_file(self, uuid: str) -> requests.Response:
        return self._request("GET", self.api_url, "/v1/files/" + quote(uuid, safe="") + "/peer_fetch", stream=True)

    def file_metadata(self, uuid: str):
        return self._json("GET", self.api_url, "/v1/files/" + quote(uuid, safe="") + "/metadata")

    def search_files(self, name: str, *, limit=100, offset=0):
        return self._json("GET", self.api_url, "/v1/files/search", params={"name": name, "limit": limit, "offset": offset})

    def file_stats(self):
        return self._json("GET", self.api_url, "/v1/files/stats")

    def delete_file(self, uuid: str):
        return self._json("DELETE", self.api_url, "/v1/files/" + quote(uuid, safe=""))

    def sync_files(self):
        return self._json("POST", self.api_url, "/v1/files/sync")

    def highlow_status(self):
        return self._json("GET", self.highlow_url, "/v1/highlow/status")

    def replay_highlow(self, scope="all", *, since_utc: datetime | None = None):
        request = {"scope": scope}
        if since_utc is not None:
            if since_utc.tzinfo is None:
                raise ValueError("since_utc must include a timezone")
            request["since_utc"] = since_utc.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")
        return self._json("POST", self.highlow_url, "/v1/highlow/replay", json_body=request)

    def highlow_replay_status(self):
        return self._json("GET", self.highlow_url, "/v1/highlow/replay/status")

    def highlow_provenance(self, records: list[dict]):
        return self._json("POST", self.highlow_url, "/v1/highlow/provenance", json_body={"records": records})

    def run_admin_command(self, command: str | dict):
        if isinstance(command, str):
            command = json.loads(command)
        result = self._json("POST", self.admin_url, "/v1/admin/commands", json_body=command)
        for response in result.get("responses", []):
            if "Error" in response:
                message = response["Error"].get("msg", "admin command failed")
                raise AdminCommandError(message, result)
        return result


def _wire(statement):
    return statement.wire() if isinstance(statement, Statement) else statement


def admin_unit(name: str) -> str:
    return json.dumps(name)


def admin_nested(group: str, variant: str, args=None) -> str:
    return json.dumps({group: variant if args is None else {variant: args}}, separators=(",", ":"))


def admin_ping(): return admin_unit("Ping")
def admin_reload_schema(): return admin_unit("Reload")
def admin_reload_dictionaries(): return admin_unit("ReloadDicts")
def admin_sync_generate(): return admin_nested("Sync", "Generate")
def admin_sync_reconcile_gaps(): return admin_nested("Sync", "ReconcileGaps")
def admin_sync_check_bookie_consistency(): return admin_nested("Sync", "CheckBookieConsistency")
def admin_sync_process_buffered_changes(actor_id, version, chunk_size):
    return admin_nested("Sync", "ProcessBufferedChanges", {"actor_id": actor_id, "version": version, "chunk_size": chunk_size})
def admin_cluster_rejoin(): return admin_nested("Cluster", "Rejoin")
def admin_cluster_members(): return admin_nested("Cluster", "Members")
def admin_cluster_membership_states(): return admin_nested("Cluster", "MembershipStates")
def admin_cluster_set_id(cluster_id): return admin_nested("Cluster", "SetId", cluster_id)
def admin_actor_version(actor_id, version): return admin_nested("Actor", "Version", {"actor_id": actor_id, "version": version})
def admin_subscriptions_list(): return admin_nested("Subs", "List")
def admin_subscription_info(query_hash=None, subscription_id=None):
    return admin_nested("Subs", "Info", {"hash": query_hash, "id": subscription_id})
def admin_set_log_filter(filter): return admin_nested("Log", "Set", {"filter": filter})
def admin_reset_log_filter(): return admin_nested("Log", "Reset")
def admin_plumtree_stats(): return admin_nested("Plumtree", "Stats")

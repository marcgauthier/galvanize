"""PostgreSQL wire access using psycopg; never opens GALVANIZE SQLite files."""

from __future__ import annotations

import os


def connect(dsn: str, **kwargs):
    """Connect via PostgreSQL wire. DSN TLS options follow libpq conventions."""
    if not dsn or not dsn.strip():
        raise ValueError("PostgreSQL DSN must not be empty")
    import psycopg

    return psycopg.connect(dsn, **kwargs)


def connect_from_env(env_name: str = "GALVANIZE_PG_DSN", **kwargs):
    """Connect using a DSN stored in a named environment variable."""
    try:
        dsn = os.environ[env_name]
    except KeyError as exc:
        raise RuntimeError(f"PostgreSQL DSN environment variable {env_name!r} is unset") from exc
    return connect(dsn, **kwargs)

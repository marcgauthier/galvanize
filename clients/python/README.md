# GALVANIZE Python client

This SDK lives beside the Go client. SQL connections use PostgreSQL wire via
`psycopg`; it never reads or writes node SQLite files. Install from this folder
with `python -m pip install -e .`.

```python
import os
from galvanize import Client, Statement, connect_from_env

with connect_from_env("GALVANIZE_PG_DSN") as conn:
    conn.execute("INSERT INTO notes(id, body) VALUES (%s, %s)", (1, "hello"))

client = Client(
    "https://db.example",
    admin_url="https://db.example:8444",
    highlow_url="https://high.example:8443",
    bearer_token=os.getenv("GALVANIZE_API_TOKEN"),
    verify="/path/to/ca.pem",
    cert=("/path/to/client.pem", "/path/to/client.key"),
)
with client.query(Statement("SELECT id, body FROM notes")) as events:
    for event in events:
        print(event)
```

`verify` accepts a CA bundle path or `True`; `cert` can supply the client
certificate/key pair for mTLS. Admin and High/Low URLs use their dedicated
mTLS listeners. `unlock_from_env` requires an HTTPS public API and reads the
database key only from a named environment variable. The API surface includes
query/transaction streams, resumable subscriptions, table updates, health and
stats, file operations, High/Low status/replay/provenance, and all admin socket
commands. Schema reload uses schema files already configured on the node;
schema upload and local backup/restore are intentionally not remote operations.

Run unit tests with `python -m unittest discover -s tests` from this directory.

# GALVANIZE Go client

The module in this directory provides a Go SDK for GALVANIZE. SQL goes through
the configured PostgreSQL wire listener with Go's `database/sql` API; HTTP
operations use the public API or the dedicated mTLS control listeners. It never
opens a node's SQLite files.

## SQL

Set `GALVANIZE_PG_DSN` to a PostgreSQL connection string. Use `sslmode=verify-full`
and `sslrootcert`, and, when the server requires client certificates, `sslcert`
and `sslkey`. Keep passwords and private keys in named environment variables or
protected files rather than source code or command-line arguments.

```go
db, err := galvanize.OpenSQLFromEnv("GALVANIZE_PG_DSN")
if err != nil { return err }
defer db.Close()

if err := db.PingContext(ctx); err != nil { return err }
rows, err := db.QueryContext(ctx, "SELECT id, body FROM notes WHERE id = $1", 1)
```

`github.com/lib/pq` registers the `postgres` `database/sql` driver used by
`OpenSQL`. GALVANIZE accepts SQLite-flavored SQL through its PostgreSQL wire
listener, so PostgreSQL-only SQL syntax may not work.

## HTTP and control APIs

```go
client, err := galvanize.NewClient(galvanize.Config{
    APIURL:      "https://db.example",
    AdminURL:    "https://db.example:8444",
    HighLowURL:  "https://high.example:8443",
    BearerToken: os.Getenv("GALVANIZE_API_TOKEN"),
    TLSConfig:   tlsConfig, // add CA roots and client certificate as needed
})
```

`APIURL` may point directly at the public HTTP API or at a TLS-terminating
reverse proxy. The admin and High/Low control URLs are dedicated mTLS listeners.
The client exposes query/transaction streams, resumable subscriptions, table
updates, health, caller-key and environment-key unlock methods, table stats, file
operations, High/Low status/replay/provenance, and every existing admin-socket command. Schema reload
uses schema files already configured on the node; this API does not upload schema
files or provide local backup/restore operations.

For bound blobs in HTTP SQL statements, use `galvanize.Blob` so bytes use the
integer-array representation expected by the API.

Run package tests with `go test ./...` from this directory. See
`../../tests-live/go-client/README.md` for the live scenario.

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

For application SQL, use `OpenSQLFromEnv` and the PostgreSQL wire listener.
The HTTP `Query` and `Transaction` methods remain available for advanced API
workflows. For bound blobs in HTTP SQL statements, raw `[]byte` and
`galvanize.Blob` use the integer-array representation expected by the API.

Stream events expose legacy `Values` (JSON numbers as `float64`) and
`ValueJSON(index)` for lossless access to large integers. `HasChangeID`
distinguishes an absent end-of-query watermark from a present zero.

Typed methods cover health, table stats, file upload/search/metadata/stats,
High/Low status/replay/provenance, and unlock. Existing raw JSON methods remain
available. Deprecated response aliases are populated for a transition release.
`ListFiles(ctx, 0, 0)` uses the server's default page size. Use
`UploadFileWithMetadata` to attach JSON metadata to encrypted files.

`TLSConfig` is the common TLS identity. Set `AdminTLSConfig` and
`HighLowTLSConfig` when control listeners require different client certificates
or trust roots. These configurations are cloned. A custom `HTTPClient` supplies
all transport behavior itself and cannot be combined with TLS config fields.
The client refuses HTTPS redirects to cleartext URLs.

## Local operations

`LocalCLI` runs the installed `galvanize` binary with argument vectors and a
context. Set `BinaryPath` explicitly and use `ConfigPath` for node operations.
Set `WorkingDir` when a CLI operation creates relative paths, such as TLS
certificate generation.
Its methods cover agent startup, backup, restore, offline rekey, Consul sync,
templates, TLS certificate generation, database locking, and change sampling.
`Run` exposes other current and future CLI subcommands; remote admin operations
are available through `RunAdminCommand`. SQL application code should use the
PostgreSQL wire listener. `Rekey` accepts key environment variable names only
and requires the new key variable to be set in the process environment.

```go
cli := galvanize.LocalCLI{BinaryPath: "/opt/galvanize/bin/galvanize", ConfigPath: "/etc/galvanize/config.toml"}
err := cli.Backup(ctx, "/var/backups/galvanize.db")
```

Run package tests with `go test ./...` from this directory. See
`../../tests-live/go-client/README.md` for the live scenario. Run
`go test -run '^$' -bench 'Benchmark(Event|Upload)' -benchmem` to measure client
decode and bulk upload costs.

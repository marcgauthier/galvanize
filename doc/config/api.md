# The [api] block

The `[api]` block configures the local Corrosion HTTP API and, optionally, a PostgreSQL wire-protocol listener.

## Required fields

### `api.addr`

Address for the Corrosion HTTP API to listen on. Accepts either a single socket address or an array of addresses if you want to listen on multiple interfaces.

`addr` is an alias for `bind_addr`; either name works in the config file.

```toml
[api]
addr = "0.0.0.0:9000"
```

## api.authz.bearer-token

### Optional fields

#### `api.endpoint_name`

This is a label used to identify nodes in the same cluster, used mostly to ensure requests aren't processed
by the wrong one. An incoming request with a different label in the `x-corrosion-requested-endpoint-name` header is rejected.

```toml
[api]
endpoint_name = "corrosion-iad-1"
```

#### `api.authz.bearer-token`

Bearer token used to authenticate HTTP requests. Clients must set this token in the `Authorization` header (`Authorization: Bearer <token>`).

```toml
[api]
authz.bearer-token = "<token>"
```

## PostgreSQL wire protocol

Corrosion can additionally expose its database over the [PostgreSQL wire protocol](../api/pg.md) for ad-hoc SQL access. The `pg` field accepts either a single listener config or an array of listener configs.

#### `api.pg.addr`

Address to listen on for PostgreSQL connections.

```toml
[api]
pg.addr = "127.0.0.1:5470"
```

Multiple listeners (e.g. one read-write, one read-only):

```toml
[[api.pg]]
addr = "127.0.0.1:5470"

[[api.pg]]
addr = "127.0.0.1:5471"
readonly = true
```

#### `api.pg.readonly`

When `true`, the listener rejects statements that would mutate the database. Defaults to `false`.

```toml
[api.pg]
addr     = "127.0.0.1:5471"
readonly = true
```

#### `api.pg.tls`

Enable TLS for incoming PostgreSQL connections.

```toml
[api.pg]
addr = "0.0.0.0:5470"

[api.pg.tls]
cert_file     = "/path/to/server_cert.pem"
key_file      = "/path/to/server_key.pem"
ca_file       = "/path/to/ca_cert.pem"   # optional
verify_client = false                    # optional, set true to require client certs
```

When `verify_client = true`, only clients presenting a certificate signed by `ca_file` will be accepted (mutual TLS).

## Remote admin control API

The local admin Unix socket remains available. To permit remote operator commands, optionally configure a separate mTLS HTTPS listener. The listener forwards commands through the existing admin socket and supports all of its commands, including schema reload from the node's configured schema paths.

```toml
[admin]
uds-path = "/run/galvanize/admin.sock"

[admin.control-api]
addr = "127.0.0.1:8444"
server-cert-env = "GALV_ADMIN_SERVER_CERT"
server-key-env = "GALV_ADMIN_SERVER_KEY"
client-ca-cert-env = "GALV_ADMIN_CLIENT_CA"
```

Each environment variable contains PEM material. `POST /v1/admin/commands` accepts one JSON command in the same externally tagged form as the admin socket protocol, such as `"Ping"`, `"Reload"`, or `{"Cluster":"Members"}`. It returns `{ "responses": [...] }` with the command's ordered log, JSON, success, or error events. Admin command errors are included as response events; HTTP 502 indicates the local admin socket could not process the request.

The endpoint is powerful: its client certificate grants the ability to run every admin-socket command, including cluster identity changes and buffered-change processing. Protect the client key and restrict network access to the listener.

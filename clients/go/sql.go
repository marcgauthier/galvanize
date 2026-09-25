package galvanize

import (
	"database/sql"
	"fmt"
	"os"
	"strings"

	_ "github.com/lib/pq"
)

// OpenSQL opens a database/sql pool using the PostgreSQL wire listener.
// The DSN supports standard lib/pq TLS options (sslmode, sslrootcert,
// sslcert, and sslkey). Supply the DSN from a named environment variable.
func OpenSQL(dsn string) (*sql.DB, error) {
	if strings.TrimSpace(dsn) == "" {
		return nil, fmt.Errorf("PostgreSQL DSN must not be empty")
	}
	return sql.Open("postgres", dsn)
}

// OpenSQLFromEnv opens a database/sql pool using a DSN from the named environment variable.
func OpenSQLFromEnv(envName string) (*sql.DB, error) {
	dsn, ok := os.LookupEnv(envName)
	if !ok {
		return nil, fmt.Errorf("PostgreSQL DSN environment variable %q is unset", envName)
	}
	return OpenSQL(dsn)
}

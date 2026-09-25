package galvanize

import (
	"context"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// TestLiveNodeSurface is enabled by tests-live/go-client/run.sh.
func TestLiveNodeSurface(t *testing.T) {
	apiURL := os.Getenv("GALVANIZE_TEST_API_URL")
	if apiURL == "" {
		t.Skip("live GALVANIZE endpoint not configured")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	rootPEM, err := os.ReadFile(os.Getenv("GALVANIZE_TEST_TLS_CA"))
	if err != nil {
		t.Fatal(err)
	}
	roots := x509.NewCertPool()
	if !roots.AppendCertsFromPEM(rootPEM) {
		t.Fatal("could not load test CA certificate")
	}
	clientCert, err := tls.LoadX509KeyPair(os.Getenv("GALVANIZE_TEST_TLS_CERT"), os.Getenv("GALVANIZE_TEST_TLS_KEY"))
	if err != nil {
		t.Fatal(err)
	}
	// Terminate HTTPS locally and proxy to the fixture's HTTP-only public
	// listener. This exercises the SDK's real HTTPS unlock path without adding
	// a TLS proxy process to the scenario script.
	publicTarget, err := url.Parse(apiURL)
	if err != nil {
		t.Fatal(err)
	}
	publicProxy := httptest.NewTLSServer(httputil.NewSingleHostReverseProxy(publicTarget))
	defer publicProxy.Close()
	roots.AddCert(publicProxy.Certificate())
	highTarget, err := url.Parse(os.Getenv("GALVANIZE_TEST_HIGH_API_URL"))
	if err != nil {
		t.Fatal(err)
	}
	highProxy := httptest.NewTLSServer(httputil.NewSingleHostReverseProxy(highTarget))
	defer highProxy.Close()
	roots.AddCert(highProxy.Certificate())
	tlsOptions := &tls.Config{RootCAs: roots, Certificates: []tls.Certificate{clientCert}, MinVersion: tls.VersionTLS12}
	client, err := NewClient(Config{
		APIURL:     publicProxy.URL,
		AdminURL:   os.Getenv("GALVANIZE_TEST_ADMIN_URL"),
		HighLowURL: os.Getenv("GALVANIZE_TEST_HIGHLOW_URL"),
		TLSConfig:  tlsOptions,
	})
	if err != nil {
		t.Fatal(err)
	}
	cipher := "chacha20"
	if _, err := client.UnlockFromEnv(ctx, "GALV_TEST_DB_KEY", &cipher, nil); err != nil {
		t.Fatalf("unlock over HTTPS: %v", err)
	}
	highClient, err := NewClient(Config{APIURL: highProxy.URL, HighLowURL: os.Getenv("GALVANIZE_TEST_HIGH_HIGHLOW_URL"), TLSConfig: tlsOptions})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := highClient.UnlockFromEnv(ctx, "GALV_TEST_HIGH_DB_KEY", &cipher, nil); err != nil {
		t.Fatalf("High unlock: %v", err)
	}
	highDB, err := OpenSQLFromEnv("GALVANIZE_TEST_HIGH_PG_DSN")
	if err != nil {
		t.Fatal(err)
	}
	defer highDB.Close()
	db, err := OpenSQLFromEnv("GALVANIZE_TEST_PG_DSN")
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	var pingErr error
	for i := 0; i < 50; i++ {
		if pingErr = db.PingContext(ctx); pingErr == nil {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	if pingErr != nil {
		t.Fatalf("database ping failed: %v", pingErr)
	}

	// Also verify the SDK refuses to send unlock material to cleartext URLs.
	cleartextClient, err := NewClient(Config{APIURL: apiURL})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := cleartextClient.UnlockFromEnv(ctx, "GALV_TEST_DB_KEY", nil, nil); err == nil || !strings.Contains(err.Error(), "requires an HTTPS") {
		t.Fatalf("expected cleartext unlock to be rejected, got %v", err)
	}

	updates, err := client.Updates(ctx, "notes")
	if err != nil {
		t.Fatal(err)
	}
	defer updates.Close()
	rowID := time.Now().UnixNano()
	if _, err := db.ExecContext(ctx, "INSERT INTO notes(id, body) VALUES($1, $2)", rowID, "from database/sql"); err != nil {
		t.Fatal(err)
	}

	// Exercise a one-shot HTTP SQL query and its row event decoding as a
	// typical app would when it needs an HTTP rather than PostgreSQL response.
	query, err := client.Query(ctx, Statement{Query: "SELECT body FROM notes WHERE id = ?", Params: []any{rowID}}, 10)
	if err != nil {
		t.Fatal(err)
	}
	queryEvent, err := query.Next()
	if err != nil || queryEvent.Type != "columns" {
		t.Fatalf("query columns event=%+v err=%v", queryEvent, err)
	}
	queryEvent, err = query.Next()
	if err != nil || queryEvent.Type != "row" || queryEvent.Values[0] != "from database/sql" {
		t.Fatalf("query row event=%+v err=%v", queryEvent, err)
	}
	if err := query.Close(); err != nil {
		t.Fatal(err)
	}

	stream, err := client.Subscribe(ctx, SQL("SELECT id, body FROM notes"), SubscribeOptions{SkipRows: false})
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	var resumeFrom uint64
	for {
		event, err := stream.Next()
		if err != nil {
			t.Fatal(err)
		}
		if event.Type == "eoq" {
			resumeFrom = event.ChangeID
			break
		}
	}
	if stream.QueryID == "" {
		t.Fatal("subscription response did not include a query ID")
	}
	resumed, err := client.ResumeSubscription(ctx, stream.QueryID, SubscribeOptions{From: &resumeFrom, SkipRows: true})
	if err != nil {
		t.Fatal(err)
	}
	defer resumed.Close()

	_, err = client.Transaction(ctx, []Statement{{Query: "INSERT INTO notes(id, body) VALUES(?, ?)", Params: []any{rowID + 1, "from HTTP"}}}, 10)
	if err != nil {
		t.Fatal(err)
	}
	for {
		event, err := stream.Next()
		if err != nil {
			t.Fatal(err)
		}
		if event.Type == "change" && event.Change == "insert" {
			break
		}
	}
	for {
		event, err := resumed.Next()
		if err != nil {
			t.Fatal(err)
		}
		if event.Type == "change" && event.Change == "insert" {
			break
		}
	}

	updateEvent, err := updates.Next()
	if err != nil || updateEvent.Type != "notify" {
		t.Fatalf("table update event=%+v err=%v", updateEvent, err)
	}
	if err := stream.Close(); err != nil {
		t.Fatal(err)
	}
	if err := updates.Close(); err != nil {
		t.Fatal(err)
	}

	stats, err := client.TableStats(ctx, []string{"notes", "missing_table"})
	if err != nil || len(stats) == 0 {
		t.Fatalf("table stats: %s, %v", stats, err)
	}
	if _, err := client.Health(ctx, url.Values{"queue_size": []string{"1000000"}}); err != nil {
		t.Fatal(err)
	}

	fileName := filepath.Base(t.TempDir()) + ".txt"
	file, err := client.UploadFile(ctx, fileName, "text/plain", strings.NewReader("go-client-file"))
	if err != nil {
		t.Fatal(err)
	}
	var uploaded struct {
		UUID string `json:"uuid"`
	}
	if err := json.Unmarshal(file, &uploaded); err != nil || uploaded.UUID == "" {
		t.Fatalf("upload response %s: %v", file, err)
	}
	fileBody, err := client.GetFile(ctx, uploaded.UUID)
	if err != nil {
		t.Fatal(err)
	}
	contents, readErr := io.ReadAll(fileBody)
	closeErr := fileBody.Close()
	if readErr != nil {
		t.Fatal(readErr)
	}
	if closeErr != nil {
		t.Fatal(closeErr)
	}
	if string(contents) != "go-client-file" {
		t.Fatalf("downloaded %q", contents)
	}
	peerBody, err := client.PeerFetchFile(ctx, uploaded.UUID)
	if err != nil {
		t.Fatal(err)
	}
	peerContents, err := io.ReadAll(peerBody)
	closeErr = peerBody.Close()
	if err != nil || closeErr != nil || string(peerContents) != "go-client-file" {
		t.Fatalf("peer-fetch bytes=%q read err=%v close err=%v", peerContents, err, closeErr)
	}
	if _, err := client.FileMetadata(ctx, uploaded.UUID); err != nil {
		t.Fatal(err)
	}
	search, err := client.SearchFiles(ctx, fileName, 10, 0)
	if err != nil || !strings.Contains(string(search), uploaded.UUID) {
		t.Fatalf("file search: %s, %v", search, err)
	}
	if _, err := client.FileStats(ctx); err != nil {
		t.Fatal(err)
	}
	if err := client.DeleteFile(ctx, uploaded.UUID); err != nil {
		t.Fatal(err)
	}

	// Exercise the caller-chosen UUID upload route separately from generated IDs.
	chosenUUID := "00000000-0000-4000-8000-000000000001"
	chosenFile, err := client.UploadFileWithUUID(ctx, chosenUUID, "chosen.txt", "text/plain", strings.NewReader("chosen-file"))
	if err != nil || !strings.Contains(string(chosenFile), chosenUUID) {
		t.Fatalf("upload with UUID response=%s err=%v", chosenFile, err)
	}
	chosenBody, err := client.GetFile(ctx, chosenUUID)
	if err != nil {
		t.Fatal(err)
	}
	chosenContents, readErr := io.ReadAll(chosenBody)
	closeErr = chosenBody.Close()
	if readErr != nil || closeErr != nil || string(chosenContents) != "chosen-file" {
		t.Fatalf("chosen file bytes=%q read err=%v close err=%v", chosenContents, readErr, closeErr)
	}
	if err := client.DeleteFile(ctx, chosenUUID); err != nil {
		t.Fatal(err)
	}
	// File sync is High-only; the Low fixture must return the documented error.
	_, err = client.SyncFiles(ctx)
	var syncErr *APIError
	if !errors.As(err, &syncErr) || syncErr.StatusCode != http.StatusInternalServerError {
		t.Fatalf("expected Low-role file sync rejection, got %v", err)
	}
	fileStats, err := client.FileStatsTyped(ctx)
	if err != nil || fileStats.LocalCachedFiles > fileStats.TotalFiles {
		t.Fatalf("typed file stats=%+v err=%v", fileStats, err)
	}
	if _, err := client.ListFiles(ctx, 0, 0); err != nil {
		t.Fatalf("list files: %v", err)
	}
	metaUpload, err := client.UploadFileBytesWithMetadata(ctx, "metadata.txt", "text/plain", map[string]string{"source": "go-client"}, []byte("metadata-file"))
	if err != nil || metaUpload.UUID == "" {
		t.Fatalf("metadata upload=%+v err=%v", metaUpload, err)
	}
	metaResult, err := client.FileMetadataTyped(ctx, metaUpload.UUID)
	if err != nil || !strings.Contains(string(metaResult.Metadata), "go-client") {
		t.Fatalf("file metadata=%+v err=%v", metaResult, err)
	}
	if err := client.DeleteFile(ctx, metaUpload.UUID); err != nil {
		t.Fatal(err)
	}

	adminResult, err := client.RunAdminCommand(ctx, AdminPing())
	if err != nil {
		t.Fatal(err)
	}
	if len(adminResult.Responses) == 0 || adminResult.Responses[len(adminResult.Responses)-1].Kind != "Success" {
		t.Fatalf("admin ping response: %+v", adminResult)
	}
	// Read-only operational commands demonstrate the other admin response
	// shapes without mutating cluster membership or process-wide log filters.
	for _, command := range []AdminCommand{AdminSubscriptionsList(), AdminClusterMembers(), AdminClusterMembershipStates()} {
		result, err := client.RunAdminCommand(ctx, command)
		if err != nil {
			t.Fatalf("admin command %s: %v", command, err)
		}
		if len(result.Responses) == 0 {
			t.Fatalf("admin command %s returned no response events", command)
		}
	}
	// Plumtree is not enabled in plaintext gossip mode; verify it returns an AdminCommandError
	if _, err := client.RunAdminCommand(ctx, AdminPlumtreeStats()); err == nil {
		t.Fatal("expected Plumtree command to return an error when disabled")
	}

	if _, err := client.RunAdminCommand(ctx, AdminReloadSchema()); err != nil {
		t.Fatalf("admin schema reload: %v", err)
	}
	if _, err := client.RunAdminCommand(ctx, AdminReloadDictionaries()); err != nil {
		t.Fatalf("admin dictionary reload: %v", err)
	}
	_, err = client.RunAdminCommand(ctx, AdminSubscriptionInfo(nil, nil))
	var adminErr *AdminCommandError
	if !errors.As(err, &adminErr) {
		t.Fatalf("expected structured admin command error for missing subscription selector, got %v", err)
	}

	status, err := client.HighLowStatus(ctx)
	if err != nil {
		t.Fatal(err)
	}
	var role struct {
		Role string `json:"role"`
	}
	if err := json.Unmarshal(status, &role); err != nil || role.Role != "low" {
		t.Fatalf("High/Low status %s: %v", status, err)
	}
	typedStatus, err := client.HighLowStatusTyped(ctx)
	if err != nil || !typedStatus.Enabled || typedStatus.Role != "low" || typedStatus.PendingExportCount == nil {
		t.Fatalf("typed High/Low status=%+v err=%v", typedStatus, err)
	}
	if _, err := client.HighLowReplayStatusTyped(ctx); err != nil {
		t.Fatal(err)
	}
	job, err := client.ReplayHighLowTyped(ctx, "all", nil)
	if err != nil || job.JobID == "" || job.Scope != "all" {
		t.Fatalf("replay job=%+v err=%v", job, err)
	}
	// Provenance is HIGH-only. The LOW fixture should reject this request with
	// 403, proving the mTLS route and role boundary are both being exercised.
	_, err = client.HighLowProvenance(ctx, []ProvenanceRecord{{Table: "notes", PrimaryKey: map[string]any{"id": rowID}}})
	var apiErr *APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != http.StatusForbidden {
		t.Fatalf("expected LOW-role provenance rejection (403), got %v", err)
	}

	var observed string
	if err := db.QueryRowContext(ctx, "SELECT body FROM notes WHERE id = $1", rowID).Scan(&observed); err != nil || observed != "from database/sql" {
		t.Fatalf("PostgreSQL wire row=%q err=%v", observed, err)
	}
	var highObserved string
	for ctx.Err() == nil {
		if err := highDB.QueryRowContext(ctx, "SELECT body FROM notes WHERE id = $1", rowID).Scan(&highObserved); err == nil && highObserved == "from database/sql" {
			break
		}
		time.Sleep(200 * time.Millisecond)
	}
	if highObserved != "from database/sql" {
		t.Fatalf("High did not import Low row: %q, context: %v", highObserved, ctx.Err())
	}
	lowDigest, err := notesDigest(ctx, db)
	if err != nil {
		t.Fatal(err)
	}
	var highDigest [32]byte
	for ctx.Err() == nil {
		highDigest, err = notesDigest(ctx, highDB)
		if err == nil && highDigest == lowDigest {
			break
		}
		time.Sleep(200 * time.Millisecond)
	}
	if err != nil {
		t.Fatal(err)
	}
	if lowDigest != highDigest {
		t.Fatalf("Low and High notes differ: %x != %x", lowDigest, highDigest)
	}
	provenance, err := highClient.HighLowProvenanceTyped(ctx, []ProvenanceRecord{{Table: "notes", PrimaryKey: map[string]any{"id": rowID}}})
	if err != nil || len(provenance.Records) != 1 || !provenance.Records[0].LowOrigin {
		t.Fatalf("High provenance=%+v err=%v", provenance, err)
	}
	if _, err := highClient.SyncFilesTyped(ctx); err != nil {
		t.Fatalf("High file sync: %v", err)
	}
	cli := LocalCLI{BinaryPath: os.Getenv("GALVANIZE_TEST_BIN"), ConfigPath: os.Getenv("GALVANIZE_TEST_CONFIG"), Stdout: io.Discard, Stderr: io.Discard}
	if err := cli.Run(ctx, "cluster", "members"); err != nil {
		t.Fatalf("local CLI cluster members: %v", err)
	}
}

// notesDigest compares application rows over PostgreSQL wire; Low and High
// maintain different internal journals, so their database files differ by design.
func notesDigest(ctx context.Context, db *sql.DB) ([32]byte, error) {
	var empty [32]byte
	rows, err := db.QueryContext(ctx, "SELECT id, body FROM notes ORDER BY id")
	if err != nil {
		return empty, err
	}
	defer rows.Close()
	hash := sha256.New()
	for rows.Next() {
		var id int64
		var body string
		if err := rows.Scan(&id, &body); err != nil {
			return empty, err
		}
		if _, err := fmt.Fprintf(hash, "%d:%d:%s\n", id, len(body), body); err != nil {
			return empty, err
		}
	}
	if err := rows.Err(); err != nil {
		return empty, err
	}
	var result [32]byte
	copy(result[:], hash.Sum(nil))
	return result, nil
}

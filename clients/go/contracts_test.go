package galvanize

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestStreamValuesKeepExactJSON(t *testing.T) {
	var event Event
	if err := json.Unmarshal([]byte(`{"row":[1,[9007199254740993,"ok"]]}`), &event); err != nil {
		t.Fatal(err)
	}
	exact, err := event.ValueJSON(0)
	if err != nil || string(exact) != "9007199254740993" {
		t.Fatalf("exact value=%s err=%v", exact, err)
	}
	if _, err := event.ValueJSON(2); err == nil {
		t.Fatal("expected index error")
	}
	if err := json.Unmarshal([]byte(`{"eoq":{"time":0.1}}`), &event); err != nil {
		t.Fatal(err)
	}
	if event.HasChangeID {
		t.Fatal("missing watermark treated as zero")
	}
	if err := json.Unmarshal([]byte(`{"eoq":{"time":0.1,"change_id":0}}`), &event); err != nil {
		t.Fatal(err)
	}
	if !event.HasChangeID {
		t.Fatal("zero watermark treated as absent")
	}
	if err := json.Unmarshal([]byte(`{"row":[1,[]],"eoq":{"time":0}}`), &event); err == nil {
		t.Fatal("accepted event with multiple variants")
	}
}

func TestNextContextInterruptsBlockedRead(t *testing.T) {
	reader, writer := io.Pipe()
	defer writer.Close()
	stream := newEventStream(&http.Response{Body: reader, Header: make(http.Header)})
	defer stream.Close()
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { _, err := stream.NextContext(ctx); done <- err }()
	time.Sleep(10 * time.Millisecond)
	cancel()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("read error=%v", err)
		}
	case <-time.After(time.Second):
		t.Fatal("blocked stream did not stop on cancellation")
	}
}

func TestProvenanceKeepsExactPrimaryKey(t *testing.T) {
	var result ProvenanceResult
	err := json.Unmarshal([]byte(`{"table":"t","primary_key":{"id":9007199254740993},"low_origin":true,"high_owned_fields":[]}`), &result)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(result.RawPrimaryKey), "9007199254740993") {
		t.Fatalf("raw primary key=%s", result.RawPrimaryKey)
	}
}

func TestTypedServerContracts(t *testing.T) {
	transport := roundTripFunc(func(r *http.Request) (*http.Response, error) {
		var body string
		switch r.URL.Path {
		case "/v1/health":
			body = `{"gaps":0,"members":1,"p99_lag":0.2,"queue_size":0}`
		case "/v1/files/search":
			if r.URL.Query().Has("name") || r.URL.Query().Has("limit") {
				t.Error("default search filters should be omitted")
			}
			body = `[{"uuid":"u","filename":"f","size":3,"sha256":"abc","content_type":null,"created_at":"now","updated_at":"now","metadata":null}]`
		case "/v1/files/upload", "/v1/files/u":
			if r.Header.Get("x-metadata") != `{"a":1}` {
				t.Errorf("metadata header=%q", r.Header.Get("x-metadata"))
			}
			body = `{"status":"ok","uuid":"u","filename":"f","size":3,"sha256":"abc","content_type":"text/plain","created_at":"now","updated_at":"now"}`
		case "/v1/files/sync":
			body = `{"status":"ok","synced_count":2}`
		case "/v1/highlow/status":
			body = `{"enabled":true,"role":"low","worker_state":"running","pending_export_count":2,"latest_export_result":null,"latest_import_result":null,"active_replay_job":null}`
		case "/v1/highlow/replay":
			body = `{"job_id":"j","state":"queued","scope":"all","since_utc":null,"end_sequence":3,"cursor_sequence":0,"total_events":3,"replayed_events":0,"bundle_count":0,"created_at_ms":1,"started_at_ms":null,"completed_at_ms":null,"error":null}`
		case "/v1/highlow/replay/status":
			body = `{"job":null,"logs":[{"at_ms":1,"level":"info","message":"started"}]}`
		case "/v1/highlow/provenance":
			body = `{"records":[{"table":"t","primary_key":{"id":1},"low_origin":true,"high_owned_fields":[],"stream":"s","low_first_applied_at_ms":1,"low_last_applied_at_ms":2,"last_high_override_at_ms":null}]}`
		default:
			t.Fatalf("unexpected route %s", r.URL.Path)
		}
		return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(body)), Request: r}, nil
	})
	c, err := NewClient(Config{APIURL: "https://test.invalid", HighLowURL: "https://control.invalid", HTTPClient: &http.Client{Transport: transport}})
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	health, err := c.HealthTyped(ctx, nil)
	if err != nil || health.Members != 1 {
		t.Fatalf("health=%+v err=%v", health, err)
	}
	files, err := c.ListFiles(ctx, 0, 0)
	if err != nil || len(files) != 1 || files[0].Filename != "f" {
		t.Fatalf("files=%+v err=%v", files, err)
	}
	upload, err := c.UploadFileBytesWithMetadata(ctx, "f", "text/plain", map[string]int{"a": 1}, []byte("abc"))
	if err != nil || upload.Size != 3 {
		t.Fatalf("upload=%+v err=%v", upload, err)
	}
	if _, err := c.UploadFileWithUUIDAndMetadata(ctx, "u", "f", "text/plain", map[string]int{"a": 1}, strings.NewReader("abc")); err != nil {
		t.Fatal(err)
	}
	syncResult, err := c.SyncFilesTyped(ctx)
	if err != nil || syncResult.SyncedCount != 2 {
		t.Fatalf("sync=%+v err=%v", syncResult, err)
	}
	status, err := c.HighLowStatusTyped(ctx)
	if err != nil || status.PendingExportCount == nil || *status.PendingExportCount != 2 || status.PendingExports != 2 {
		t.Fatalf("status=%+v err=%v", status, err)
	}
	job, err := c.ReplayHighLowTyped(ctx, "all", nil)
	if err != nil || job.State != "queued" || job.Status != "queued" || job.TotalEvents != 3 {
		t.Fatalf("job=%+v err=%v", job, err)
	}
	replay, err := c.HighLowReplayStatusTyped(ctx)
	if err != nil || len(replay.Logs) != 1 {
		t.Fatalf("replay=%+v err=%v", replay, err)
	}
	provenance, err := c.HighLowProvenanceTyped(ctx, []ProvenanceRecord{{Table: "t", PrimaryKey: map[string]any{"id": 1}}})
	if err != nil || len(provenance.Records) != 1 || !provenance.Records[0].LowOrigin {
		t.Fatalf("provenance=%+v err=%v", provenance, err)
	}
}

func TestLocalCLIUsesArgumentVectorAndKeyNames(t *testing.T) {
	dir := t.TempDir()
	script := filepath.Join(dir, "galvanize")
	if err := os.WriteFile(script, []byte("#!/bin/sh\nprintf '%s\\n' \"$@\"\n"), 0700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GALV_NEW_KEY", "secret-key-value")
	var output strings.Builder
	cli := LocalCLI{BinaryPath: script, ConfigPath: "/tmp/config with spaces.toml", Stdout: &output}
	if err := cli.Rekey(context.Background(), "/tmp/db with spaces", "", "GALV_NEW_KEY", "", ""); err != nil {
		t.Fatal(err)
	}
	got := output.String()
	if !strings.Contains(got, "/tmp/db with spaces\n") || !strings.Contains(got, "GALV_NEW_KEY\n") || strings.Contains(got, "secret-key-value") {
		t.Fatalf("unsafe CLI arguments: %q", got)
	}
	for _, test := range []struct {
		name string
		run  func() error
		want string
	}{
		{"backup", func() error { return cli.Backup(context.Background(), "/tmp/out db") }, "backup\n/tmp/out db\n"},
		{"restore", func() error { return cli.Restore(context.Background(), "/tmp/in db", true, "") }, "restore\n/tmp/in db\n--self-actor-id\n"},
		{"consul", func() error { return cli.ConsulSync(context.Background()) }, "consul\nsync\n"},
		{"template", func() error { return cli.Template(context.Background(), true, "a:b") }, "template\n--once\na:b\n"},
		{"ca", func() error { return cli.GenerateCA(context.Background()) }, "tls\nca\ngenerate\n"},
		{"server cert", func() error { return cli.GenerateServerCert(context.Background(), "127.0.0.1", "key", "cert") }, "tls\nserver\ngenerate\n127.0.0.1\n--ca-key\nkey\n--ca-cert\ncert\n"},
		{"client cert", func() error { return cli.GenerateClientCert(context.Background(), "key", "cert") }, "tls\nclient\ngenerate\n--ca-key\nkey\n--ca-cert\ncert\n"},
		{"sample changes", func() error { return cli.SampleChanges(context.Background(), "/tmp/samples", 10) }, "db\nsample-changes\n--out\n/tmp/samples\n--limit\n10\n"},
		{"db lock", func() error { return cli.LockDB(context.Background(), "true") }, "db\nlock\ntrue\n"},
	} {
		t.Run(test.name, func(t *testing.T) {
			output.Reset()
			if err := test.run(); err != nil {
				t.Fatal(err)
			}
			if !strings.Contains(output.String(), test.want) {
				t.Fatalf("args=%q want suffix %q", output.String(), test.want)
			}
		})
	}
}

func TestAPIErrorBodyIsBounded(t *testing.T) {
	transport := roundTripFunc(func(r *http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: 500, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(strings.Repeat("x", maxAPIErrorBytes*2))), Request: r}, nil
	})
	c, err := NewClient(Config{APIURL: "http://test.invalid", HTTPClient: &http.Client{Transport: transport}})
	if err != nil {
		t.Fatal(err)
	}
	_, err = c.Health(context.Background(), nil)
	apiErr, ok := err.(*APIError)
	if !ok || len(apiErr.Body) > maxAPIErrorBytes+20 {
		t.Fatalf("error=%v", err)
	}
}

func TestSeparateControlTLS(t *testing.T) {
	api := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.WriteString(w, `{"gaps":0,"members":1,"p99_lag":0,"queue_size":0}`)
	}))
	defer api.Close()
	admin := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.WriteString(w, `{"responses":[{"Success":null}]}`)
	}))
	defer admin.Close()
	apiRoots, adminRoots := x509.NewCertPool(), x509.NewCertPool()
	apiRoots.AddCert(api.Certificate())
	adminRoots.AddCert(admin.Certificate())
	c, err := NewClient(Config{APIURL: api.URL, AdminURL: admin.URL, TLSConfig: &tls.Config{RootCAs: apiRoots}, AdminTLSConfig: &tls.Config{RootCAs: adminRoots}})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := c.HealthTyped(context.Background(), nil); err != nil {
		t.Fatal(err)
	}
	if err := c.Ping(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, err := NewClient(Config{APIURL: "https://user:password@test.invalid"}); err == nil {
		t.Fatal("accepted credentials in URL")
	}
}

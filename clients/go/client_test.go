package galvanize

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"
)

type roundTripFunc func(*http.Request) (*http.Response, error)

func (fn roundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) { return fn(request) }

func TestStatementJSON(t *testing.T) {
	cases := []struct {
		name  string
		value Statement
		want  string
	}{
		{"simple", SQL("SELECT 1"), `"SELECT 1"`},
		{"bound_blob", Statement{Query: "SELECT ?", Params: []any{int64(7), Blob{0, 255}}}, ` ["SELECT ?",[7,[0,255]]]`},
		{"bound_raw_bytes", NewBoundStatement("SELECT ?", int64(7), []byte{0, 255}), ` ["SELECT ?",[7,[0,255]]]`},
		{"named_blob", Statement{Query: "SELECT :value", NamedParams: map[string]any{"value": "x"}}, `{"named_params":{"value":"x"},"query":"SELECT :value"}`},
		{"named_raw_bytes", NewNamedStatement("SELECT :b", map[string]any{"b": []byte{1, 2}}), `{"named_params":{"b":[1,2]},"query":"SELECT :b"}`},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			encoded, err := json.Marshal(test.value)
			if err != nil {
				t.Fatal(err)
			}
			want := strings.TrimSpace(test.want)
			if string(encoded) != want {
				t.Fatalf("got %s, want %s", encoded, want)
			}
		})
	}
}

func TestEventHelpers(t *testing.T) {
	evCol := Event{Type: EventTypeColumns}
	if !evCol.IsColumns() || evCol.IsRow() {
		t.Errorf("unexpected event helper result for columns")
	}

	evRow := Event{Type: EventTypeRow}
	if !evRow.IsRow() || evRow.IsChange() {
		t.Errorf("unexpected event helper result for row")
	}

	evChange := Event{Type: EventTypeChange}
	if !evChange.IsChange() || evChange.IsEOQ() {
		t.Errorf("unexpected event helper result for change")
	}

	evEOQ := Event{Type: EventTypeEOQ}
	if !evEOQ.IsEOQ() || evEOQ.IsError() {
		t.Errorf("unexpected event helper result for eoq")
	}

	evErr := Event{Type: EventTypeError}
	if !evErr.IsError() || evErr.IsNotify() {
		t.Errorf("unexpected event helper result for error")
	}

	evNotify := Event{Type: EventTypeNotify}
	if !evNotify.IsNotify() || evNotify.IsColumns() {
		t.Errorf("unexpected event helper result for notify")
	}
}

func TestQueryAndTransaction(t *testing.T) {
	transport := roundTripFunc(func(r *http.Request) (*http.Response, error) {
		if r.URL.Path != "/v1/admin/commands" && r.Header.Get("Authorization") != "Bearer secret" {
			t.Errorf("missing bearer token")
		}
		if r.URL.Path == "/v1/admin/commands" && r.Header.Get("Authorization") != "" {
			t.Errorf("public bearer token sent to admin control API")
		}
		switch r.URL.Path {
		case "/v1/queries":
			return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/x-ndjson"}}, Body: io.NopCloser(strings.NewReader("{\"columns\":[\"id\"]}\n{\"row\":[1,[9]]}\n{\"eoq\":{\"time\":0.01}}\n")), Request: r}, nil
		case "/v1/transactions":
			if got := r.URL.Query().Get("timeout"); got != "4" {
				t.Errorf("timeout=%q", got)
			}
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"results":[{"rows_affected":1,"time":0.01}],"time":0.02,"version":2,"actor_id":"node"}`)), Request: r}, nil
		case "/v1/files/upload":
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"uuid":"123","filename":"test.txt"}`)), Request: r}, nil
		case "/v1/files/123":
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("file-content")), Request: r}, nil
		case "/v1/table_stats":
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"total_row_count":42,"invalid_tables":[]}`)), Request: r}, nil
		case "/v1/files/123/metadata":
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"uuid":"123","filename":"test.txt","size":12}`)), Request: r}, nil
		case "/v1/files/stats":
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"total_files":1,"total_bytes":12,"local_cached_files":1,"local_cached_bytes":12,"missing_local_files":0}`)), Request: r}, nil
		case "/v1/admin/commands":
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"responses":[{"Success":null}]}`)), Request: r}, nil
		default:
			return &http.Response{StatusCode: http.StatusNotFound, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("not found")), Request: r}, nil
		}
	})

	client, err := NewClient(Config{APIURL: "http://db.test", AdminURL: "http://db.test", BearerToken: "secret", HTTPClient: &http.Client{Transport: transport}})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := client.Query(context.Background(), SQL("SELECT id"), 0)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	columns, err := stream.Next()
	if err != nil || !columns.IsColumns() || len(columns.Columns) != 1 {
		t.Fatalf("columns event=%+v, err=%v", columns, err)
	}
	row, err := stream.Next()
	if err != nil || !row.IsRow() || row.RowID != 1 || row.Values[0].(float64) != 9 {
		t.Fatalf("row event=%+v, err=%v", row, err)
	}

	result, err := client.Transaction(context.Background(), []Statement{SQL("INSERT INTO t VALUES (1)")}, 4)
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Results) != 1 || result.Results[0].RowsAffected != 1 || result.Version == nil || *result.Version != 2 {
		t.Fatalf("unexpected transaction response: %+v", result)
	}

	// Test high level file methods
	uploadRes, err := client.UploadFileBytes(context.Background(), "test.txt", "text/plain", []byte("file-content"))
	if err != nil || !strings.Contains(string(uploadRes), "123") {
		t.Fatalf("upload failed: %s, %v", uploadRes, err)
	}

	downloadBytes, err := client.DownloadFileBytes(context.Background(), "123")
	if err != nil || string(downloadBytes) != "file-content" {
		t.Fatalf("download failed: %s, %v", downloadBytes, err)
	}

	// Test typed methods
	tStats, err := client.TableStatsTyped(context.Background(), []string{"notes"})
	if err != nil || tStats.TotalRowCount != 42 {
		t.Fatalf("TableStatsTyped failed: %+v, %v", tStats, err)
	}

	fMeta, err := client.FileMetadataTyped(context.Background(), "123")
	if err != nil || fMeta.Filename != "test.txt" || fMeta.Size != 12 {
		t.Fatalf("FileMetadataTyped failed: %+v, %v", fMeta, err)
	}

	fStats, err := client.FileStatsTyped(context.Background())
	if err != nil || fStats.TotalFiles != 1 || fStats.TotalBytes != 12 || fStats.LocalCachedFiles != 1 || fStats.LocalFiles != 1 {
		t.Fatalf("FileStatsTyped failed: %+v, %v", fStats, err)
	}

	// Test Admin shortcut methods
	if err := client.Ping(context.Background()); err != nil {
		t.Fatalf("Ping failed: %v", err)
	}
	if err := client.ReloadSchema(context.Background()); err != nil {
		t.Fatalf("ReloadSchema failed: %v", err)
	}
	if err := client.SyncGenerate(context.Background()); err != nil {
		t.Fatalf("SyncGenerate failed: %v", err)
	}
}

func TestEventStreamContextCancellation(t *testing.T) {
	pipeR, pipeW := io.Pipe()
	transport := roundTripFunc(func(r *http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: pipeR, Request: r}, nil
	})

	client, err := NewClient(Config{APIURL: "http://db.test", HTTPClient: &http.Client{Transport: transport}})
	if err != nil {
		t.Fatal(err)
	}

	stream, err := client.Query(context.Background(), SQL("SELECT 1"), 0)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	ctx, cancel := context.WithCancel(context.Background())
	cancel() // cancel immediately

	_, err = stream.NextContext(ctx)
	if err == nil || !strings.Contains(err.Error(), "context canceled") {
		t.Fatalf("expected context canceled error, got %v", err)
	}
	_ = pipeW.Close()
}

func TestAdminCommandAndResponses(t *testing.T) {
	command := AdminClusterSetID(42)
	if got, want := string(command), `{"Cluster":{"SetId":42}}`; got != want {
		t.Fatalf("got %s, want %s", got, want)
	}
	var response AdminResponse
	if err := json.Unmarshal([]byte(`{"Error":{"msg":"denied"}}`), &response); err != nil {
		t.Fatal(err)
	}
	if response.Kind != "Error" || response.Error != "denied" {
		t.Fatalf("unexpected response: %+v", response)
	}
}

func TestUnlockRequiresHTTPS(t *testing.T) {
	client, err := NewClient(Config{APIURL: "http://localhost:8080"})
	if err != nil {
		t.Fatal(err)
	}
	if err := client.Unlock(context.Background(), "database-key"); err == nil || !strings.Contains(err.Error(), "requires an HTTPS") {
		t.Fatalf("expected HTTPS requirement, got %v", err)
	}
	if _, err := client.UnlockFromEnv(context.Background(), "MISSING_DB_KEY", nil, nil); err == nil || !strings.Contains(err.Error(), "requires an HTTPS") {
		t.Fatalf("expected HTTPS requirement, got %v", err)
	}
}

func TestUnlockUsesCallerKeyAndAcceptsAlreadyUnlockedNode(t *testing.T) {
	requests := 0
	transport := roundTripFunc(func(r *http.Request) (*http.Response, error) {
		requests++
		switch r.URL.Path {
		case "/v1/admin/unlock":
			var payload map[string]string
			if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
				t.Fatal(err)
			}
			if len(payload) != 1 || payload["key"] != "database-key" {
				t.Errorf("unlock payload = %#v", payload)
			}
			return &http.Response{StatusCode: http.StatusConflict, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("already unlocked")), Request: r}, nil
		case "/v1/health":
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"status":"ready"}`)), Request: r}, nil
		default:
			t.Errorf("unexpected request path %s", r.URL.Path)
			return &http.Response{StatusCode: http.StatusNotFound, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("not found")), Request: r}, nil
		}
	})
	client, err := NewClient(Config{APIURL: "https://db.test", HTTPClient: &http.Client{Transport: transport}})
	if err != nil {
		t.Fatal(err)
	}
	if err := client.Unlock(context.Background(), "database-key"); err != nil {
		t.Fatalf("unlock already unlocked node: %v", err)
	}
	if requests != 2 {
		t.Fatalf("request count = %d, want unlock plus health", requests)
	}
}

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
		{"bound", Statement{Query: "SELECT ?", Params: []any{int64(7), Blob{0, 255}}}, ` ["SELECT ?",[7,[0,255]]]`},
		{"named", Statement{Query: "SELECT :value", NamedParams: map[string]any{"value": "x"}}, `{"named_params":{"value":"x"},"query":"SELECT :value"}`},
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

func TestQueryAndTransaction(t *testing.T) {
	transport := roundTripFunc(func(r *http.Request) (*http.Response, error) {
		if r.Header.Get("Authorization") != "Bearer secret" {
			t.Errorf("missing bearer token")
		}
		switch r.URL.Path {
		case "/v1/queries":
			return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/x-ndjson"}}, Body: io.NopCloser(strings.NewReader("{\"columns\":[\"id\"]}\n{\"row\":[1,[9]]}\n{\"eoq\":{\"time\":0.01}}\n")), Request: r}, nil
		case "/v1/transactions":
			if got := r.URL.Query().Get("timeout"); got != "4" {
				t.Errorf("timeout=%q", got)
			}
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"results":[{"rows_affected":1,"time":0.01}],"time":0.02,"version":2,"actor_id":"node"}`)), Request: r}, nil
		default:
			return &http.Response{StatusCode: http.StatusNotFound, Header: make(http.Header), Body: io.NopCloser(strings.NewReader("not found")), Request: r}, nil
		}
	})

	client, err := NewClient(Config{APIURL: "http://db.test", BearerToken: "secret", HTTPClient: &http.Client{Transport: transport}})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := client.Query(context.Background(), SQL("SELECT id"), 0)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	columns, err := stream.Next()
	if err != nil || columns.Type != "columns" || len(columns.Columns) != 1 {
		t.Fatalf("columns event=%+v, err=%v", columns, err)
	}
	row, err := stream.Next()
	if err != nil || row.Type != "row" || row.RowID != 1 || row.Values[0].(float64) != 9 {
		t.Fatalf("row event=%+v, err=%v", row, err)
	}

	result, err := client.Transaction(context.Background(), []Statement{SQL("INSERT INTO t VALUES (1)")}, 4)
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Results) != 1 || result.Results[0].RowsAffected != 1 || result.Version == nil || *result.Version != 2 {
		t.Fatalf("unexpected transaction response: %+v", result)
	}
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

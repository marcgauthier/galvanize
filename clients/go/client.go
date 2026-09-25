// Package galvanize provides Go clients for GALVANIZE's PostgreSQL and HTTP APIs.
package galvanize

import (
	"bufio"
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"
)

// Config configures the public API and the optional dedicated control APIs.
// TLSConfig is cloned before use. Set the matching listener URLs to enable the
// admin and High/Low control methods.
type Config struct {
	APIURL      string
	AdminURL    string
	HighLowURL  string
	BearerToken string
	TLSConfig   *tls.Config
	HTTPClient  *http.Client
}

// Client talks to the public HTTP API and optional control APIs. SQL is exposed
// separately through database/sql; see OpenSQL and the package README.
type Client struct {
	apiURL     string
	adminURL   string
	highLowURL string
	token      string
	http       *http.Client
}

// NewClient creates a client. APIURL, AdminURL, and HighLowURL must be HTTP(S)
// URLs when provided.
func NewClient(config Config) (*Client, error) {
	apiURL, err := normalizeBaseURL(config.APIURL)
	if err != nil {
		return nil, fmt.Errorf("API URL: %w", err)
	}
	adminURL, err := optionalBaseURL(config.AdminURL)
	if err != nil {
		return nil, fmt.Errorf("admin URL: %w", err)
	}
	highLowURL, err := optionalBaseURL(config.HighLowURL)
	if err != nil {
		return nil, fmt.Errorf("High/Low URL: %w", err)
	}
	httpClient := config.HTTPClient
	if httpClient == nil {
		transport := http.DefaultTransport.(*http.Transport).Clone()
		if config.TLSConfig != nil {
			transport.TLSClientConfig = config.TLSConfig.Clone()
		}
		httpClient = &http.Client{Transport: transport, Timeout: 0}
	}
	return &Client{
		apiURL: apiURL, adminURL: adminURL, highLowURL: highLowURL,
		token: config.BearerToken, http: httpClient,
	}, nil
}

func normalizeBaseURL(raw string) (string, error) {
	if strings.TrimSpace(raw) == "" {
		return "", errors.New("URL is required")
	}
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		return "", fmt.Errorf("expected an http or https URL with a host")
	}
	u.Path = strings.TrimRight(u.Path, "/")
	u.RawQuery = ""
	u.Fragment = ""
	return strings.TrimRight(u.String(), "/"), nil
}

func optionalBaseURL(raw string) (string, error) {
	if raw == "" {
		return "", nil
	}
	return normalizeBaseURL(raw)
}

// APIError carries a non-success HTTP response, including its response body.
type APIError struct {
	StatusCode int
	Body       []byte
}

func (e *APIError) Error() string {
	message := strings.TrimSpace(string(e.Body))
	if message == "" {
		return fmt.Sprintf("GALVANIZE API returned HTTP %d", e.StatusCode)
	}
	return fmt.Sprintf("GALVANIZE API returned HTTP %d: %s", e.StatusCode, message)
}

func (c *Client) request(ctx context.Context, method, endpoint string, query url.Values, body io.Reader, contentType string, base string) (*http.Response, error) {
	if base == "" {
		return nil, errors.New("this API endpoint is not configured")
	}
	requestURL := base + endpoint
	if len(query) > 0 {
		requestURL += "?" + query.Encode()
	}
	req, err := http.NewRequestWithContext(ctx, method, requestURL, body)
	if err != nil {
		return nil, err
	}
	if contentType != "" {
		req.Header.Set("Content-Type", contentType)
	}
	if c.token != "" && base == c.apiURL {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		defer resp.Body.Close()
		responseBody, readErr := io.ReadAll(resp.Body)
		if readErr != nil {
			return nil, readErr
		}
		return nil, &APIError{StatusCode: resp.StatusCode, Body: responseBody}
	}
	return resp, nil
}

func (c *Client) jsonRequest(ctx context.Context, method, endpoint string, query url.Values, input, output any, base string) error {
	var body io.Reader
	if input != nil {
		encoded, err := json.Marshal(input)
		if err != nil {
			return err
		}
		body = bytes.NewReader(encoded)
	}
	resp, err := c.request(ctx, method, endpoint, query, body, "application/json", base)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if output == nil {
		_, err = io.Copy(io.Discard, resp.Body)
		return err
	}
	return json.NewDecoder(resp.Body).Decode(output)
}

// Statement describes one SQLite-flavored SQL statement and optional bindings.
// Use Blob for binary parameters because GALVANIZE's JSON wire format represents
// byte arrays as arrays of integer values.
type Statement struct {
	Query       string
	Params      []any
	NamedParams map[string]any
}

func SQL(query string) Statement { return Statement{Query: query} }

// Blob is a binary SQL parameter encoded using GALVANIZE's integer-array JSON form.
type Blob []byte

func (b Blob) MarshalJSON() ([]byte, error) {
	values := make([]int, len(b))
	for index, value := range b {
		values[index] = int(value)
	}
	return json.Marshal(values)
}

func (s Statement) MarshalJSON() ([]byte, error) {
	if s.Query == "" {
		return nil, errors.New("SQL query must not be empty")
	}
	if s.NamedParams != nil {
		return json.Marshal(map[string]any{"query": s.Query, "named_params": s.NamedParams})
	}
	if s.Params != nil {
		return json.Marshal([]any{s.Query, s.Params})
	}
	return json.Marshal(s.Query)
}

// ExecResult is the per-statement result returned by a transaction.
type ExecResult struct {
	RowsAffected int64   `json:"rows_affected"`
	Time         float64 `json:"time"`
	Error        string  `json:"error,omitempty"`
}

// ExecResponse describes the outcome of an atomic HTTP transaction.
type ExecResponse struct {
	Results []ExecResult `json:"results"`
	Time    float64      `json:"time"`
	Version *uint64      `json:"version"`
	ActorID *string      `json:"actor_id"`
}

// Transaction executes statements atomically. timeoutSeconds is optional and
// is passed through to the node's transaction timeout.
func (c *Client) Transaction(ctx context.Context, statements []Statement, timeoutSeconds uint64) (ExecResponse, error) {
	var result ExecResponse
	query := make(url.Values)
	if timeoutSeconds > 0 {
		query.Set("timeout", fmt.Sprint(timeoutSeconds))
	}
	err := c.jsonRequest(ctx, http.MethodPost, "/v1/transactions", query, statements, &result, c.apiURL)
	return result, err
}

// Event is one query, subscription, or table-update NDJSON event. Fields not
// used by a given event type are left at their zero values.
type Event struct {
	Type     string          `json:"-"`
	Columns  []string        `json:"-"`
	RowID    uint64          `json:"-"`
	Values   []any           `json:"-"`
	Change   string          `json:"-"`
	ChangeID uint64          `json:"-"`
	Time     float64         `json:"-"`
	Error    string          `json:"-"`
	Raw      json.RawMessage `json:"-"`
}

func (e *Event) UnmarshalJSON(data []byte) error {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	for name, raw := range fields {
		e.Type = name
		e.Raw = append(e.Raw[:0], raw...)
		switch name {
		case "columns":
			return json.Unmarshal(raw, &e.Columns)
		case "row":
			var tuple []json.RawMessage
			if err := json.Unmarshal(raw, &tuple); err != nil || len(tuple) != 2 {
				return errors.New("invalid row event")
			}
			return decodeRow(tuple[0], tuple[1], &e.RowID, &e.Values)
		case "change":
			var tuple []json.RawMessage
			if err := json.Unmarshal(raw, &tuple); err != nil || len(tuple) != 4 {
				return errors.New("invalid change event")
			}
			if err := json.Unmarshal(tuple[0], &e.Change); err != nil {
				return err
			}
			if err := decodeRow(tuple[1], tuple[2], &e.RowID, &e.Values); err != nil {
				return err
			}
			return json.Unmarshal(tuple[3], &e.ChangeID)
		case "eoq":
			var value struct {
				Time     float64 `json:"time"`
				ChangeID uint64  `json:"change_id"`
			}
			if err := json.Unmarshal(raw, &value); err != nil {
				return err
			}
			e.Time, e.ChangeID = value.Time, value.ChangeID
			return nil
		case "error":
			return json.Unmarshal(raw, &e.Error)
		case "notify":
			var tuple []json.RawMessage
			if err := json.Unmarshal(raw, &tuple); err != nil || len(tuple) != 2 {
				return errors.New("invalid notify event")
			}
			if err := json.Unmarshal(tuple[0], &e.Change); err != nil {
				return err
			}
			return json.Unmarshal(tuple[1], &e.Values)
		default:
			return fmt.Errorf("unknown GALVANIZE event %q", name)
		}
	}
	return errors.New("empty GALVANIZE event")
}

func decodeRow(idRaw, valuesRaw []byte, id *uint64, values *[]any) error {
	if err := json.Unmarshal(idRaw, id); err != nil {
		return err
	}
	return json.Unmarshal(valuesRaw, values)
}

// EventStream reads one NDJSON event at a time. Close it when the consumer stops.
type EventStream struct {
	body      io.ReadCloser
	reader    *bufio.Reader
	QueryID   string
	QueryHash string
}

func newEventStream(resp *http.Response) *EventStream {
	return &EventStream{
		body:      resp.Body,
		reader:    bufio.NewReader(resp.Body),
		QueryID:   resp.Header.Get("corro-query-id"),
		QueryHash: resp.Header.Get("corro-query-hash"),
	}
}

// Next reads the next event, returning io.EOF when the stream closes.
func (s *EventStream) Next() (Event, error) {
	for {
		line, err := s.reader.ReadBytes('\n')
		if len(bytes.TrimSpace(line)) == 0 {
			if err != nil {
				return Event{}, err
			}
			continue
		}
		var event Event
		if decodeErr := json.Unmarshal(line, &event); decodeErr != nil {
			return Event{}, decodeErr
		}
		return event, nil
	}
}

func (s *EventStream) Close() error { return s.body.Close() }

// Query streams a one-shot query result from POST /v1/queries.
func (c *Client) Query(ctx context.Context, statement Statement, timeoutSeconds uint64) (*EventStream, error) {
	query := make(url.Values)
	if timeoutSeconds > 0 {
		query.Set("timeout", fmt.Sprint(timeoutSeconds))
	}
	resp, err := c.jsonStreamRequest(ctx, http.MethodPost, "/v1/queries", query, statement, c.apiURL)
	if err != nil {
		return nil, err
	}
	return newEventStream(resp), nil
}

// Subscribe starts a live query. Pass from to resume from a change ID.
type SubscribeOptions struct {
	From     *uint64
	SkipRows bool
}

func (c *Client) Subscribe(ctx context.Context, statement Statement, options SubscribeOptions) (*EventStream, error) {
	query := make(url.Values)
	query.Set("skip_rows", fmt.Sprint(options.SkipRows))
	if options.From != nil {
		query.Set("from", fmt.Sprint(*options.From))
	}
	resp, err := c.jsonStreamRequest(ctx, http.MethodPost, "/v1/subscriptions", query, statement, c.apiURL)
	if err != nil {
		return nil, err
	}
	return newEventStream(resp), nil
}

// ResumeSubscription reconnects to an existing subscription.
func (c *Client) ResumeSubscription(ctx context.Context, id string, options SubscribeOptions) (*EventStream, error) {
	query := make(url.Values)
	query.Set("skip_rows", fmt.Sprint(options.SkipRows))
	if options.From != nil {
		query.Set("from", fmt.Sprint(*options.From))
	}
	resp, err := c.request(ctx, http.MethodGet, "/v1/subscriptions/"+url.PathEscape(id), query, nil, "", c.apiURL)
	if err != nil {
		return nil, err
	}
	return newEventStream(resp), nil
}

// Updates streams primary-key notifications from one CR-SQLite table.
func (c *Client) Updates(ctx context.Context, table string) (*EventStream, error) {
	resp, err := c.request(ctx, http.MethodPost, "/v1/updates/"+url.PathEscape(table), nil, nil, "", c.apiURL)
	if err != nil {
		return nil, err
	}
	return newEventStream(resp), nil
}

func (c *Client) jsonStreamRequest(ctx context.Context, method, endpoint string, query url.Values, input any, base string) (*http.Response, error) {
	encoded, err := json.Marshal(input)
	if err != nil {
		return nil, err
	}
	return c.request(ctx, method, endpoint, query, bytes.NewReader(encoded), "application/json", base)
}

// TableStats returns row counts and invalid table names.
func (c *Client) TableStats(ctx context.Context, tables []string) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodPost, "/v1/table_stats", nil, map[string]any{"tables": tables}, c.apiURL)
}

// Health returns the node's health document. Thresholds use the endpoint's
// query names (gaps, max_queue, p99_lag, queue_size, failure_status).
func (c *Client) Health(ctx context.Context, thresholds url.Values) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodGet, "/v1/health", thresholds, nil, c.apiURL)
}

// UnlockFromEnv submits a database key read from the named environment variable.
// It requires HTTPS to avoid sending the key over a clear-text connection.
func (c *Client) UnlockFromEnv(ctx context.Context, keyEnv string, cipher, cipherParams *string) (json.RawMessage, error) {
	if !strings.HasPrefix(c.apiURL, "https://") {
		return nil, errors.New("database unlock requires an HTTPS API URL")
	}
	key, ok := os.LookupEnv(keyEnv)
	if !ok {
		return nil, fmt.Errorf("database key environment variable %q is unset", keyEnv)
	}
	return c.rawJSON(ctx, http.MethodPost, "/v1/admin/unlock", nil, map[string]any{"key": key, "cipher": cipher, "cipher_params": cipherParams}, c.apiURL)
}

func (c *Client) rawJSON(ctx context.Context, method, endpoint string, query url.Values, input any, base string) (json.RawMessage, error) {
	var result json.RawMessage
	err := c.jsonRequest(ctx, method, endpoint, query, input, &result, base)
	return result, err
}

// UploadFile sends raw file bytes and returns the server's metadata response.
func (c *Client) UploadFile(ctx context.Context, filename, contentType string, contents io.Reader) (json.RawMessage, error) {
	query := url.Values{"filename": []string{filename}}
	resp, err := c.request(ctx, http.MethodPost, "/v1/files/upload", query, contents, contentType, c.apiURL)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var result json.RawMessage
	err = json.NewDecoder(resp.Body).Decode(&result)
	return result, err
}

// UploadFileWithUUID stores bytes under a caller-supplied UUID.
func (c *Client) UploadFileWithUUID(ctx context.Context, uuid, filename, contentType string, contents io.Reader) (json.RawMessage, error) {
	query := url.Values{"filename": []string{filename}}
	if contentType != "" {
		query.Set("content_type", contentType)
	}
	resp, err := c.request(ctx, http.MethodPost, "/v1/files/"+url.PathEscape(uuid), query, contents, contentType, c.apiURL)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var result json.RawMessage
	err = json.NewDecoder(resp.Body).Decode(&result)
	return result, err
}

// GetFile downloads decrypted bytes and leaves response-body closure to the caller.
func (c *Client) GetFile(ctx context.Context, uuid string) (io.ReadCloser, error) {
	resp, err := c.request(ctx, http.MethodGet, "/v1/files/"+url.PathEscape(uuid), nil, nil, "", c.apiURL)
	if err != nil {
		return nil, err
	}
	return resp.Body, nil
}

// PeerFetchFile retrieves locally cached bytes through the peer-fetch route.
func (c *Client) PeerFetchFile(ctx context.Context, uuid string) (io.ReadCloser, error) {
	resp, err := c.request(ctx, http.MethodGet, "/v1/files/"+url.PathEscape(uuid)+"/peer_fetch", nil, nil, "", c.apiURL)
	if err != nil {
		return nil, err
	}
	return resp.Body, nil
}

// FileMetadata returns metadata for one file UUID.
func (c *Client) FileMetadata(ctx context.Context, uuid string) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodGet, "/v1/files/"+url.PathEscape(uuid)+"/metadata", nil, nil, c.apiURL)
}

// SearchFiles searches the replicated file metadata catalog.
func (c *Client) SearchFiles(ctx context.Context, name string, limit, offset uint64) (json.RawMessage, error) {
	query := url.Values{"name": []string{name}, "limit": []string{fmt.Sprint(limit)}, "offset": []string{fmt.Sprint(offset)}}
	return c.rawJSON(ctx, http.MethodGet, "/v1/files/search", query, nil, c.apiURL)
}

// FileStats returns total and locally cached file counts and sizes.
func (c *Client) FileStats(ctx context.Context) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodGet, "/v1/files/stats", nil, nil, c.apiURL)
}

// DeleteFile removes replicated metadata and this node's cached file bytes.
func (c *Client) DeleteFile(ctx context.Context, uuid string) error {
	return c.jsonRequest(ctx, http.MethodDelete, "/v1/files/"+url.PathEscape(uuid), nil, nil, nil, c.apiURL)
}

// SyncFiles triggers one High/Low file-fetch cycle.
func (c *Client) SyncFiles(ctx context.Context) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodPost, "/v1/files/sync", nil, nil, c.apiURL)
}

// HighLowStatus returns High/Low worker and import/export status.
func (c *Client) HighLowStatus(ctx context.Context) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodGet, "/v1/highlow/status", nil, nil, c.highLowURL)
}

// ReplayHighLow enqueues replay of all retained events or events since an RFC3339 UTC time.
func (c *Client) ReplayHighLow(ctx context.Context, scope string, sinceUTC *time.Time) (json.RawMessage, error) {
	request := map[string]any{"scope": scope}
	if sinceUTC != nil {
		request["since_utc"] = sinceUTC.UTC().Format(time.RFC3339Nano)
	}
	return c.rawJSON(ctx, http.MethodPost, "/v1/highlow/replay", nil, request, c.highLowURL)
}

// HighLowReplayStatus returns the latest replay job and its recent logs.
func (c *Client) HighLowReplayStatus(ctx context.Context) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodGet, "/v1/highlow/replay/status", nil, nil, c.highLowURL)
}

// HighLowProvenance asks the authoritative High node for record provenance.
func (c *Client) HighLowProvenance(ctx context.Context, records []ProvenanceRecord) (json.RawMessage, error) {
	return c.rawJSON(ctx, http.MethodPost, "/v1/highlow/provenance", nil, map[string]any{"records": records}, c.highLowURL)
}

// ProvenanceRecord identifies a row by table and primary-key values.
type ProvenanceRecord struct {
	Table      string         `json:"table"`
	PrimaryKey map[string]any `json:"primary_key"`
}

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

const maxAPIErrorBytes = 64 << 10

// Config configures the public API and the optional dedicated control APIs.
// TLSConfig is cloned before use. Set the matching listener URLs to enable the
// admin and High/Low control methods.
type Config struct {
	APIURL           string
	AdminURL         string
	HighLowURL       string
	BearerToken      string
	TLSConfig        *tls.Config
	HTTPClient       *http.Client
	AdminTLSConfig   *tls.Config
	HighLowTLSConfig *tls.Config
}

// Client talks to the public HTTP API and optional control APIs. SQL is exposed
// separately through database/sql; see OpenSQL and the package README.
type Client struct {
	apiURL      string
	adminURL    string
	highLowURL  string
	token       string
	http        *http.Client
	adminHTTP   *http.Client
	highLowHTTP *http.Client
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
	if config.HTTPClient != nil && (config.TLSConfig != nil || config.AdminTLSConfig != nil || config.HighLowTLSConfig != nil) {
		return nil, errors.New("HTTPClient and TLSConfig fields cannot be combined")
	}
	httpClient := config.HTTPClient
	if httpClient == nil {
		httpClient = newHTTPClient(config.TLSConfig)
	}
	adminHTTP, highLowHTTP := httpClient, httpClient
	if config.HTTPClient == nil {
		if config.AdminTLSConfig != nil {
			adminHTTP = newHTTPClient(config.AdminTLSConfig)
		}
		if config.HighLowTLSConfig != nil {
			highLowHTTP = newHTTPClient(config.HighLowTLSConfig)
		}
	}
	return &Client{
		apiURL: apiURL, adminURL: adminURL, highLowURL: highLowURL,
		token: config.BearerToken, http: httpClient, adminHTTP: adminHTTP, highLowHTTP: highLowHTTP,
	}, nil
}

func newHTTPClient(tlsConfig *tls.Config) *http.Client {
	transport := http.DefaultTransport.(*http.Transport).Clone()
	if tlsConfig != nil {
		transport.TLSClientConfig = tlsConfig.Clone()
	}
	return &http.Client{Transport: transport, CheckRedirect: func(req *http.Request, via []*http.Request) error {
		if len(via) > 0 && via[0].URL.Scheme == "https" && req.URL.Scheme != "https" {
			return errors.New("HTTPS redirect to cleartext URL refused")
		}
		if len(via) >= 10 {
			return errors.New("too many redirects")
		}
		return nil
	}}
}

func normalizeBaseURL(raw string) (string, error) {
	if strings.TrimSpace(raw) == "" {
		return "", errors.New("URL is required")
	}
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" || u.User != nil {
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
	return c.requestWithHeaders(ctx, method, endpoint, query, body, contentType, base, nil)
}

func (c *Client) requestWithHeaders(ctx context.Context, method, endpoint string, query url.Values, body io.Reader, contentType string, base string, headers http.Header) (*http.Response, error) {
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
	for name, values := range headers {
		for _, value := range values {
			req.Header.Add(name, value)
		}
	}
	if c.token != "" && base == c.apiURL && endpoint != "/v1/admin/commands" && !strings.HasPrefix(endpoint, "/v1/highlow/") {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}
	httpClient := c.http
	if endpoint == "/v1/admin/commands" {
		httpClient = c.adminHTTP
	}
	if strings.HasPrefix(endpoint, "/v1/highlow/") {
		httpClient = c.highLowHTTP
	}
	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		defer resp.Body.Close()
		responseBody, readErr := io.ReadAll(io.LimitReader(resp.Body, maxAPIErrorBytes+1))
		if readErr != nil {
			return nil, readErr
		}
		if len(responseBody) > maxAPIErrorBytes {
			responseBody = append(responseBody[:maxAPIErrorBytes], []byte("... [truncated]")...)
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
// Binary parameters ([]byte or Blob) are automatically encoded using GALVANIZE's
// integer-array JSON representation.
type Statement struct {
	Query       string
	Params      []any
	NamedParams map[string]any
}

// SQL returns a simple statement without bound parameters.
func SQL(query string) Statement { return Statement{Query: query} }

// NewSimpleStatement returns a statement without bound parameters.
func NewSimpleStatement(query string) Statement { return Statement{Query: query} }

// NewBoundStatement returns a statement with positional bound parameters.
func NewBoundStatement(query string, params ...any) Statement {
	return Statement{Query: query, Params: params}
}

// NewNamedStatement returns a statement with named bound parameters.
func NewNamedStatement(query string, namedParams map[string]any) Statement {
	return Statement{Query: query, NamedParams: namedParams}
}

// Blob is a binary SQL parameter encoded using GALVANIZE's integer-array JSON form.
type Blob []byte

func (b Blob) MarshalJSON() ([]byte, error) {
	values := make([]int, len(b))
	for index, value := range b {
		values[index] = int(value)
	}
	return json.Marshal(values)
}

func normalizeParamValue(v any) any {
	if v == nil {
		return nil
	}
	switch val := v.(type) {
	case Blob:
		return val
	case []byte:
		return Blob(val)
	case []any:
		res := make([]any, len(val))
		for i, item := range val {
			res[i] = normalizeParamValue(item)
		}
		return res
	case map[string]any:
		res := make(map[string]any, len(val))
		for k, item := range val {
			res[k] = normalizeParamValue(item)
		}
		return res
	default:
		return v
	}
}

func (s Statement) MarshalJSON() ([]byte, error) {
	if s.Query == "" {
		return nil, errors.New("SQL query must not be empty")
	}
	if s.NamedParams != nil {
		normalized := make(map[string]any, len(s.NamedParams))
		for k, v := range s.NamedParams {
			normalized[k] = normalizeParamValue(v)
		}
		return json.Marshal(map[string]any{"query": s.Query, "named_params": normalized})
	}
	if s.Params != nil {
		normalized := make([]any, len(s.Params))
		for i, v := range s.Params {
			normalized[i] = normalizeParamValue(v)
		}
		return json.Marshal([]any{s.Query, normalized})
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

// Event type constants emitted by streaming endpoints.
const (
	EventTypeColumns = "columns"
	EventTypeRow     = "row"
	EventTypeChange  = "change"
	EventTypeEOQ     = "eoq"
	EventTypeError   = "error"
	EventTypeNotify  = "notify"
)

// Event is one query, subscription, or table-update NDJSON event. Fields not
// used by a given event type are left at their zero values.
type Event struct {
	Type     string   `json:"-"`
	Columns  []string `json:"-"`
	RowID    uint64   `json:"-"`
	Values   []any    `json:"-"`
	Change   string   `json:"-"`
	ChangeID uint64   `json:"-"`
	// HasChangeID distinguishes an absent end-of-query watermark from zero.
	HasChangeID bool            `json:"-"`
	Time        float64         `json:"-"`
	Error       string          `json:"-"`
	Raw         json.RawMessage `json:"-"`
}

func (e Event) IsColumns() bool { return e.Type == EventTypeColumns }
func (e Event) IsRow() bool     { return e.Type == EventTypeRow }
func (e Event) IsChange() bool  { return e.Type == EventTypeChange }
func (e Event) IsEOQ() bool     { return e.Type == EventTypeEOQ }
func (e Event) IsError() bool   { return e.Type == EventTypeError }
func (e Event) IsNotify() bool  { return e.Type == EventTypeNotify }

func (e *Event) UnmarshalJSON(data []byte) error {
	*e = Event{}
	var fields struct {
		Columns json.RawMessage `json:"columns"`
		Row     json.RawMessage `json:"row"`
		Change  json.RawMessage `json:"change"`
		EOQ     json.RawMessage `json:"eoq"`
		Error   json.RawMessage `json:"error"`
		Notify  json.RawMessage `json:"notify"`
	}
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	var name string
	var raw json.RawMessage
	for _, candidate := range [...]struct {
		name string
		raw  json.RawMessage
	}{{"columns", fields.Columns}, {"row", fields.Row}, {"change", fields.Change}, {"eoq", fields.EOQ}, {"error", fields.Error}, {"notify", fields.Notify}} {
		if candidate.raw == nil {
			continue
		}
		if name != "" {
			return errors.New("invalid GALVANIZE event envelope")
		}
		name, raw = candidate.name, candidate.raw
	}
	if name != "" {
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
			if err := json.Unmarshal(tuple[3], &e.ChangeID); err != nil {
				return err
			}
			e.HasChangeID = true
			return nil
		case "eoq":
			var value struct {
				Time     float64 `json:"time"`
				ChangeID *uint64 `json:"change_id"`
			}
			if err := json.Unmarshal(raw, &value); err != nil {
				return err
			}
			e.Time = value.Time
			if value.ChangeID != nil {
				e.ChangeID, e.HasChangeID = *value.ChangeID, true
			}
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
		}
	}
	return errors.New("unknown or empty GALVANIZE event")
}

func decodeRow(idRaw, valuesRaw []byte, id *uint64, values *[]any) error {
	if err := json.Unmarshal(idRaw, id); err != nil {
		return err
	}
	return json.Unmarshal(valuesRaw, values)
}

// ValueJSON returns a row, change, or notification value without losing integer
// precision. Values remains available for callers expecting the legacy float64
// decoding of JSON numbers.
func (e Event) ValueJSON(index int) (json.RawMessage, error) {
	valueIndex := 1
	switch e.Type {
	case EventTypeChange:
		valueIndex = 2
	case EventTypeRow, EventTypeNotify:
	default:
		return nil, errors.New("event has no values")
	}
	var tuple []json.RawMessage
	if err := json.Unmarshal(e.Raw, &tuple); err != nil {
		return nil, err
	}
	if len(tuple) <= valueIndex {
		return nil, errors.New("invalid event values")
	}
	var values []json.RawMessage
	if err := json.Unmarshal(tuple[valueIndex], &values); err != nil {
		return nil, err
	}
	if index < 0 || index >= len(values) {
		return nil, fmt.Errorf("value index %d out of range", index)
	}
	return values[index], nil
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
	return s.NextContext(context.Background())
}

// NextContext reads the next event, respecting context cancellation.
func (s *EventStream) NextContext(ctx context.Context) (Event, error) {
	if err := ctx.Err(); err != nil {
		return Event{}, err
	}
	if ctx.Done() == nil {
		return s.readNext()
	}
	stop := context.AfterFunc(ctx, func() { _ = s.body.Close() })
	defer stop()
	event, err := s.readNext()
	if ctx.Err() != nil {
		return Event{}, ctx.Err()
	}
	return event, err
}

func (s *EventStream) readNext() (Event, error) {
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

// Unlock submits a database key supplied by the caller. It requires HTTPS to
// avoid sending the key over a clear-text connection.
func (c *Client) Unlock(ctx context.Context, key string) error {
	if !strings.HasPrefix(c.apiURL, "https://") {
		return errors.New("database unlock requires an HTTPS API URL")
	}
	if key == "" {
		return errors.New("database key must not be empty")
	}
	err := c.jsonRequest(ctx, http.MethodPost, "/v1/admin/unlock", nil, map[string]any{"key": key}, nil, c.apiURL)
	var apiErr *APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != http.StatusConflict {
		return err
	}
	status, healthErr := c.Health(ctx, nil)
	if healthErr != nil {
		return healthErr
	}
	var response struct {
		Status string `json:"status"`
	}
	if unmarshalErr := json.Unmarshal(status, &response); unmarshalErr != nil {
		return unmarshalErr
	}
	if response.Status != "awaiting_unlock" {
		return nil
	}
	return err
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
	query := make(url.Values)
	if name != "" {
		query.Set("name", name)
	}
	if limit != 0 {
		query.Set("limit", fmt.Sprint(limit))
	}
	if offset != 0 {
		query.Set("offset", fmt.Sprint(offset))
	}
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

// DownloadFileBytes downloads the decrypted bytes for a file UUID into memory.
func (c *Client) DownloadFileBytes(ctx context.Context, uuid string) ([]byte, error) {
	rc, err := c.GetFile(ctx, uuid)
	if err != nil {
		return nil, err
	}
	defer rc.Close()
	return io.ReadAll(rc)
}

// UploadFileBytes sends raw file bytes and returns the server's metadata response.
func (c *Client) UploadFileBytes(ctx context.Context, filename, contentType string, data []byte) (json.RawMessage, error) {
	return c.UploadFile(ctx, filename, contentType, bytes.NewReader(data))
}

// UnlockResponse is the response returned when a database is unlocked.
type UnlockResponse struct {
	Status string `json:"status"`
}

// TableStatsResponse contains row counts and any un-replicated or invalid table names.
type TableStatsResponse struct {
	TotalRowCount int64    `json:"total_row_count"`
	InvalidTables []string `json:"invalid_tables,omitempty"`
}

// FileMetadataResponse contains metadata for a replicated file.
type FileMetadataResponse struct {
	Status      string          `json:"status,omitempty"`
	UUID        string          `json:"uuid"`
	Filename    string          `json:"filename"`
	Size        int64           `json:"size"`
	SHA256      string          `json:"sha256"`
	ContentType string          `json:"content_type"`
	CreatedAt   string          `json:"created_at"`
	UpdatedAt   string          `json:"updated_at"`
	Metadata    json.RawMessage `json:"metadata,omitempty"`
	IsLocal     bool            `json:"is_local,omitempty"`
}

// FileStatsResponse contains total and locally cached file metrics.
type FileStatsResponse struct {
	TotalFiles        int64 `json:"total_files"`
	TotalBytes        int64 `json:"total_bytes"`
	LocalCachedFiles  int64 `json:"local_cached_files"`
	LocalCachedBytes  int64 `json:"local_cached_bytes"`
	MissingLocalFiles int64 `json:"missing_local_files"`
	// Deprecated: use LocalCachedFiles and LocalCachedBytes.
	LocalFiles int64 `json:"-"`
	LocalBytes int64 `json:"-"`
}

func (r *FileStatsResponse) UnmarshalJSON(data []byte) error {
	type wire FileStatsResponse
	var value wire
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	*r = FileStatsResponse(value)
	r.LocalFiles, r.LocalBytes = r.LocalCachedFiles, r.LocalCachedBytes
	return nil
}

// HighLowStatusResponse contains status information for the High/Low air-gap worker.
type HighLowStatusResponse struct {
	Enabled            bool            `json:"enabled"`
	Role               string          `json:"role"`
	WorkerState        string          `json:"worker_state"`
	PendingExportCount *int64          `json:"pending_export_count"`
	ActiveReplayJob    json.RawMessage `json:"active_replay_job"`
	LatestImportResult json.RawMessage `json:"latest_import_result"`
	LatestExportResult json.RawMessage `json:"latest_export_result"`
	// Deprecated: use the matching fields above.
	PendingExports int64           `json:"-"`
	ActiveReplay   json.RawMessage `json:"-"`
	LatestImport   json.RawMessage `json:"-"`
	LatestExport   json.RawMessage `json:"-"`
}

func (r *HighLowStatusResponse) UnmarshalJSON(data []byte) error {
	type wire HighLowStatusResponse
	var value wire
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	*r = HighLowStatusResponse(value)
	if r.PendingExportCount != nil {
		r.PendingExports = *r.PendingExportCount
	}
	r.ActiveReplay, r.LatestImport, r.LatestExport = r.ActiveReplayJob, r.LatestImportResult, r.LatestExportResult
	return nil
}

// ReplayJobResponse describes an air-gap event replay job.
type ReplayJobResponse struct {
	JobID          string  `json:"job_id"`
	State          string  `json:"state"`
	Scope          string  `json:"scope"`
	SinceUTC       string  `json:"since_utc"`
	EndSequence    int64   `json:"end_sequence"`
	CursorSequence int64   `json:"cursor_sequence"`
	TotalEvents    int64   `json:"total_events"`
	ReplayedEvents int64   `json:"replayed_events"`
	BundleCount    int64   `json:"bundle_count"`
	CreatedAtMS    int64   `json:"created_at_ms"`
	StartedAtMS    *int64  `json:"started_at_ms"`
	CompletedAtMS  *int64  `json:"completed_at_ms"`
	Error          *string `json:"error"`
	// Deprecated: use State.
	Status string `json:"-"`
}

func (r *ReplayJobResponse) UnmarshalJSON(data []byte) error {
	type wire ReplayJobResponse
	var value wire
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	*r = ReplayJobResponse(value)
	r.Status = r.State
	return nil
}

// UnlockFromEnvTyped unlocks the database using a key from a named environment variable and returns a typed UnlockResponse.
func (c *Client) UnlockFromEnvTyped(ctx context.Context, keyEnv string, cipher, cipherParams *string) (UnlockResponse, error) {
	raw, err := c.UnlockFromEnv(ctx, keyEnv, cipher, cipherParams)
	if err != nil {
		return UnlockResponse{}, err
	}
	var res UnlockResponse
	err = json.Unmarshal(raw, &res)
	return res, err
}

// TableStatsTyped returns typed row counts and invalid table names.
func (c *Client) TableStatsTyped(ctx context.Context, tables []string) (TableStatsResponse, error) {
	var res TableStatsResponse
	err := c.jsonRequest(ctx, http.MethodPost, "/v1/table_stats", nil, map[string]any{"tables": tables}, &res, c.apiURL)
	return res, err
}

// FileMetadataTyped returns typed metadata for one file UUID.
func (c *Client) FileMetadataTyped(ctx context.Context, uuid string) (FileMetadataResponse, error) {
	var res FileMetadataResponse
	err := c.jsonRequest(ctx, http.MethodGet, "/v1/files/"+url.PathEscape(uuid)+"/metadata", nil, nil, &res, c.apiURL)
	return res, err
}

// FileStatsTyped returns typed file metrics.
func (c *Client) FileStatsTyped(ctx context.Context) (FileStatsResponse, error) {
	var res FileStatsResponse
	err := c.jsonRequest(ctx, http.MethodGet, "/v1/files/stats", nil, nil, &res, c.apiURL)
	return res, err
}

// HighLowStatusTyped returns typed High/Low worker status.
func (c *Client) HighLowStatusTyped(ctx context.Context) (HighLowStatusResponse, error) {
	var res HighLowStatusResponse
	err := c.jsonRequest(ctx, http.MethodGet, "/v1/highlow/status", nil, nil, &res, c.highLowURL)
	return res, err
}

// ReplayHighLowTyped enqueues an event replay job and returns a typed ReplayJobResponse.
func (c *Client) ReplayHighLowTyped(ctx context.Context, scope string, sinceUTC *time.Time) (ReplayJobResponse, error) {
	var res ReplayJobResponse
	request := map[string]any{"scope": scope}
	if sinceUTC != nil {
		request["since_utc"] = sinceUTC.UTC().Format(time.RFC3339Nano)
	}
	err := c.jsonRequest(ctx, http.MethodPost, "/v1/highlow/replay", nil, request, &res, c.highLowURL)
	return res, err
}

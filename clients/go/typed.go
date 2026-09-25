package galvanize

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"
)

// HealthResponse represents either a locked node or the current health metrics.
type HealthResponse struct {
	Status    string  `json:"status"`
	DBPath    string  `json:"db_path"`
	Gaps      int64   `json:"gaps"`
	Members   int64   `json:"members"`
	P99Lag    float64 `json:"p99_lag"`
	QueueSize uint64  `json:"queue_size"`
}

// UploadFileResponse is the metadata returned after storing a file.
type UploadFileResponse struct {
	Status      string  `json:"status"`
	UUID        string  `json:"uuid"`
	Filename    string  `json:"filename"`
	Size        uint64  `json:"size"`
	SHA256      string  `json:"sha256"`
	ContentType *string `json:"content_type"`
	CreatedAt   string  `json:"created_at"`
	UpdatedAt   string  `json:"updated_at"`
}

type FileSyncResponse struct {
	Status      string `json:"status"`
	SyncedCount uint64 `json:"synced_count"`
}

// ReplayStatusResponse contains the latest replay job and recent worker logs.
type ReplayStatusResponse struct {
	Job  *ReplayJobResponse `json:"job"`
	Logs []ReplayLog        `json:"logs"`
}

type ReplayLog struct {
	AtMS    int64  `json:"at_ms"`
	Level   string `json:"level"`
	Message string `json:"message"`
}

type ProvenanceResponse struct {
	Records []ProvenanceResult `json:"records"`
}

type ProvenanceResult struct {
	Table      string         `json:"table"`
	PrimaryKey map[string]any `json:"primary_key"`
	// RawPrimaryKey preserves large integer keys that legacy PrimaryKey decodes as float64.
	RawPrimaryKey        json.RawMessage `json:"-"`
	LowOrigin            bool            `json:"low_origin"`
	HighOwnedFields      json.RawMessage `json:"high_owned_fields"`
	Stream               *string         `json:"stream"`
	LowFirstAppliedAtMS  *int64          `json:"low_first_applied_at_ms"`
	LowLastAppliedAtMS   *int64          `json:"low_last_applied_at_ms"`
	LastHighOverrideAtMS *int64          `json:"last_high_override_at_ms"`
}

func (r *ProvenanceResult) UnmarshalJSON(data []byte) error {
	type wire ProvenanceResult
	var value wire
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	*r = ProvenanceResult(value)
	var object map[string]json.RawMessage
	if err := json.Unmarshal(data, &object); err != nil {
		return err
	}
	r.RawPrimaryKey = append(r.RawPrimaryKey[:0], object["primary_key"]...)
	return nil
}

func (c *Client) HealthTyped(ctx context.Context, thresholds url.Values) (HealthResponse, error) {
	var result HealthResponse
	err := c.jsonRequest(ctx, http.MethodGet, "/v1/health", thresholds, nil, &result, c.apiURL)
	return result, err
}

// ListFiles lists the replicated catalog without a filename filter.
func (c *Client) ListFiles(ctx context.Context, limit, offset uint64) ([]FileMetadataResponse, error) {
	return c.SearchFilesTyped(ctx, "", limit, offset)
}

func (c *Client) SearchFilesTyped(ctx context.Context, name string, limit, offset uint64) ([]FileMetadataResponse, error) {
	raw, err := c.SearchFiles(ctx, name, limit, offset)
	if err != nil {
		return nil, err
	}
	var result []FileMetadataResponse
	err = json.Unmarshal(raw, &result)
	return result, err
}

// UploadFileWithMetadata passes optional JSON metadata using the server's
// x-metadata request header. The body is streamed from contents.
func (c *Client) UploadFileWithMetadata(ctx context.Context, filename, contentType string, metadata any, contents io.Reader) (UploadFileResponse, error) {
	return c.uploadFileWithMetadata(ctx, "/v1/files/upload", filename, contentType, metadata, contents)
}

// UploadFileWithUUIDAndMetadata stores a file under a caller-selected UUID
// while attaching JSON metadata.
func (c *Client) UploadFileWithUUIDAndMetadata(ctx context.Context, uuid, filename, contentType string, metadata any, contents io.Reader) (UploadFileResponse, error) {
	return c.uploadFileWithMetadata(ctx, "/v1/files/"+url.PathEscape(uuid), filename, contentType, metadata, contents)
}

func (c *Client) uploadFileWithMetadata(ctx context.Context, endpoint, filename, contentType string, metadata any, contents io.Reader) (UploadFileResponse, error) {
	var result UploadFileResponse
	encoded, err := json.Marshal(metadata)
	if err != nil {
		return result, err
	}
	query := url.Values{"filename": []string{filename}}
	resp, err := c.requestWithHeaders(ctx, http.MethodPost, endpoint, query, contents, contentType, c.apiURL, http.Header{"X-Metadata": []string{string(encoded)}})
	if err != nil {
		return result, err
	}
	defer resp.Body.Close()
	err = json.NewDecoder(resp.Body).Decode(&result)
	return result, err
}

func (c *Client) UploadFileTyped(ctx context.Context, filename, contentType string, contents io.Reader) (UploadFileResponse, error) {
	raw, err := c.UploadFile(ctx, filename, contentType, contents)
	if err != nil {
		return UploadFileResponse{}, err
	}
	var result UploadFileResponse
	err = json.Unmarshal(raw, &result)
	return result, err
}

func (c *Client) UploadFileWithUUIDTyped(ctx context.Context, uuid, filename, contentType string, contents io.Reader) (UploadFileResponse, error) {
	raw, err := c.UploadFileWithUUID(ctx, uuid, filename, contentType, contents)
	if err != nil {
		return UploadFileResponse{}, err
	}
	var result UploadFileResponse
	err = json.Unmarshal(raw, &result)
	return result, err
}

func (c *Client) SyncFilesTyped(ctx context.Context) (FileSyncResponse, error) {
	var result FileSyncResponse
	err := c.jsonRequest(ctx, http.MethodPost, "/v1/files/sync", nil, nil, &result, c.apiURL)
	return result, err
}

func (c *Client) HighLowReplayStatusTyped(ctx context.Context) (ReplayStatusResponse, error) {
	var result ReplayStatusResponse
	err := c.jsonRequest(ctx, http.MethodGet, "/v1/highlow/replay/status", nil, nil, &result, c.highLowURL)
	return result, err
}

func (c *Client) HighLowProvenanceTyped(ctx context.Context, records []ProvenanceRecord) (ProvenanceResponse, error) {
	var result ProvenanceResponse
	err := c.jsonRequest(ctx, http.MethodPost, "/v1/highlow/provenance", nil, map[string]any{"records": records}, &result, c.highLowURL)
	return result, err
}

// UploadFileBytesWithMetadata is a convenience method for already-buffered data.
func (c *Client) UploadFileBytesWithMetadata(ctx context.Context, filename, contentType string, metadata any, data []byte) (UploadFileResponse, error) {
	return c.UploadFileWithMetadata(ctx, filename, contentType, metadata, bytes.NewReader(data))
}

package galvanize

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"testing"
)

func BenchmarkEventDecode(b *testing.B) {
	line := []byte(`{"change":["update",1,[9007199254740993,"example",true],42]}`)
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		var event Event
		if err := json.Unmarshal(line, &event); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkEventStreamNextContext(b *testing.B) {
	line := []byte(`{"row":[1,[42,"example"]]}` + "\n")
	data := bytes.Repeat(line, b.N)
	stream := &EventStream{body: io.NopCloser(bytes.NewReader(data)), reader: nil}
	stream.reader = newEventStream(&http.Response{Body: stream.body, Header: make(http.Header)}).reader
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, err := stream.NextContext(ctx); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkUploadFile(b *testing.B) {
	payload := bytes.Repeat([]byte("x"), 1<<20)
	transport := roundTripFunc(func(r *http.Request) (*http.Response, error) {
		if _, err := io.Copy(io.Discard, r.Body); err != nil {
			return nil, err
		}
		return &http.Response{StatusCode: 200, Header: make(http.Header), Body: io.NopCloser(bytes.NewReader([]byte(`{"uuid":"u"}`))), Request: r}, nil
	})
	client, err := NewClient(Config{APIURL: "http://test.invalid", HTTPClient: &http.Client{Transport: transport}})
	if err != nil {
		b.Fatal(err)
	}
	b.SetBytes(int64(len(payload)))
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		if _, err := client.UploadFile(context.Background(), "x", "application/octet-stream", bytes.NewReader(payload)); err != nil {
			b.Fatal(err)
		}
	}
}

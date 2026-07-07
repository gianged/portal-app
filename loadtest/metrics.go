package main

import (
	"fmt"
	"sort"
	"sync"
	"time"
)

// Hist collects latency samples; percentiles are computed exactly at summary
// time (sample counts here are small enough to sort in memory).
type Hist struct {
	mu      sync.Mutex
	samples []time.Duration
}

func (h *Hist) Add(d time.Duration) {
	h.mu.Lock()
	h.samples = append(h.samples, d)
	h.mu.Unlock()
}

func (h *Hist) Len() int {
	h.mu.Lock()
	defer h.mu.Unlock()
	return len(h.samples)
}

// Percentile returns the p-th (0..100) percentile, or 0 with no samples.
func (h *Hist) Percentile(p float64) time.Duration {
	h.mu.Lock()
	defer h.mu.Unlock()
	if len(h.samples) == 0 {
		return 0
	}
	sorted := make([]time.Duration, len(h.samples))
	copy(sorted, h.samples)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i] < sorted[j] })
	idx := int(float64(len(sorted)-1) * p / 100)
	return sorted[idx]
}

func (h *Hist) Summary() string {
	return fmt.Sprintf("n=%d p50=%v p90=%v p95=%v p99=%v max=%v",
		h.Len(), h.Percentile(50), h.Percentile(90), h.Percentile(95),
		h.Percentile(99), h.Percentile(100))
}

// thresholds accumulates pass/fail assertions; Err returns the first failure
// so the process exits non-zero (CI-friendly, mirrors k6 semantics).
type thresholds struct{ failures []string }

func (t *thresholds) require(ok bool, format string, args ...any) {
	line := fmt.Sprintf(format, args...)
	mark := "ok  "
	if !ok {
		mark = "FAIL"
		t.failures = append(t.failures, line)
	}
	fmt.Printf("  [%s] %s\n", mark, line)
}

func (t *thresholds) Err() error {
	if len(t.failures) > 0 {
		return fmt.Errorf("%d threshold(s) failed", len(t.failures))
	}
	return nil
}

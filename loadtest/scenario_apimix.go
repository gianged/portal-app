package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"net/http/cookiejar"
	"sync"
	"sync/atomic"
	"time"
)

// Authenticated REST mix at a ramping request rate: profile reads, channel
// list, notifications, announcements. 429 is an expected response (the
// per-user limiter working); 5xx/transport failures are not.
//
// Stress above ~2 req/s/user needs API_RATE_LIMIT raised on the server.
func runAPIMix(args []string) error {
	fs := flag.NewFlagSet("api-mix", flag.ExitOnError)
	baseURL := fs.String("base-url", defaultBaseURL, "server base URL")
	usersFile := fs.String("users", "users.json", "seeded emails file")
	password := fs.String("password", "admin123", "shared seed password")
	peak := fs.Int("peak-rps", 200, "peak request rate")
	poolSize := fs.Int("sessions", 200, "logged-in session pool size")
	hold := fs.Duration("hold", 3*time.Minute, "time at peak after the 2m ramp")
	_ = fs.Parse(args)

	users, err := loadUsers(*usersFile)
	if err != nil {
		return err
	}
	transport := newTransport()

	// Pool of independent sessions, each with its own cookie jar; a free-list
	// channel hands them to dispatched requests.
	free := make(chan *apiSession, *poolSize)
	for i := 0; i < *poolSize; i++ {
		jar, _ := cookiejar.New(nil)
		free <- &apiSession{
			id:     i,
			email:  users[i%len(users)],
			client: &http.Client{Transport: transport, Jar: jar, Timeout: 10 * time.Second},
		}
	}

	var (
		hist        Hist
		okCount     atomic.Int64
		rateLimited atomic.Int64
		serverErr   atomic.Int64
		otherFail   atomic.Int64
		dropped     atomic.Int64
		wg          sync.WaitGroup
	)

	// Ramp shape mirrors the original harness: 50 -> peak/2 over 1m,
	// -> peak over the next 1m, hold at peak.
	stages := []struct {
		from, to float64
		duration time.Duration
	}{
		{50, float64(*peak) / 2, time.Minute},
		{float64(*peak) / 2, float64(*peak), time.Minute},
		{float64(*peak), float64(*peak), *hold},
	}
	fmt.Printf("api-mix: ramp 50 -> %d rps, hold %v, %d sessions, %s\n", *peak, *hold, *poolSize, *baseURL)

	dispatch := func() {
		select {
		case s := <-free:
			wg.Add(1)
			go func() {
				defer wg.Done()
				defer func() { free <- s }()
				status, d, err := s.request(*baseURL, *password)
				hist.Add(d)
				switch {
				case err != nil || status >= 500:
					serverErr.Add(1)
				case status == http.StatusTooManyRequests:
					rateLimited.Add(1)
				case status >= 400:
					otherFail.Add(1)
				default:
					okCount.Add(1)
				}
			}()
		default:
			// Pool exhausted: the generator, not the server, is the bottleneck.
			dropped.Add(1)
		}
	}

	const tick = 20 * time.Millisecond
	for _, stage := range stages {
		start := time.Now()
		var due float64
		last := start
		for time.Since(start) < stage.duration {
			time.Sleep(tick)
			now := time.Now()
			frac := time.Since(start).Seconds() / stage.duration.Seconds()
			rate := stage.from + (stage.to-stage.from)*min(frac, 1)
			due += rate * now.Sub(last).Seconds()
			last = now
			for due >= 1 {
				due--
				dispatch()
			}
		}
	}
	wg.Wait()

	total := okCount.Load() + rateLimited.Load() + serverErr.Load() + otherFail.Load()
	fmt.Printf("requests: %d total | %d ok, %d rate-limited(429), %d server-err, %d other-fail, %d dropped\n",
		total, okCount.Load(), rateLimited.Load(), serverErr.Load(), otherFail.Load(), dropped.Load())
	fmt.Printf("latency: %s\n", hist.Summary())

	var t thresholds
	t.require(hist.Percentile(95) < 500*time.Millisecond, "p95 < 500ms (got %v)", hist.Percentile(95))
	t.require(hist.Percentile(99) < 1500*time.Millisecond, "p99 < 1.5s (got %v)", hist.Percentile(99))
	t.require(float64(serverErr.Load()) < 0.01*float64(total), "server-error rate < 1%% (got %d/%d)", serverErr.Load(), total)
	t.require(float64(otherFail.Load()) < 0.01*float64(total), "unexpected-status rate < 1%% (got %d/%d)", otherFail.Load(), total)
	return t.Err()
}

type apiSession struct {
	id        int
	email     string
	client    *http.Client
	loggedIn  bool
	generalID string
}

// request logs in on first use (resolving the general channel for the
// announcements path), then issues one random authenticated GET.
func (s *apiSession) request(baseURL, password string) (int, time.Duration, error) {
	if !s.loggedIn {
		start := time.Now()
		_, status, err := login(s.client, baseURL, s.email, password)
		if err != nil || status != http.StatusOK {
			return status, time.Since(start), err
		}
		s.loggedIn = true
		s.generalID = s.findGeneralChannel(baseURL)
		return status, time.Since(start), nil
	}

	paths := []string{
		"/api/v1/me",
		"/api/v1/chat/channels",
		"/api/v1/notifications",
		"/api/v1/notifications/unread-count",
	}
	if s.generalID != "" {
		paths = append(paths, "/api/v1/announcements?channel="+s.generalID)
	}
	path := paths[rand.Intn(len(paths))]

	start := time.Now()
	resp, err := s.client.Get(baseURL + path)
	d := time.Since(start)
	if err != nil {
		return 0, d, err
	}
	_, _ = io.Copy(io.Discard, resp.Body)
	resp.Body.Close()
	return resp.StatusCode, d, nil
}

func (s *apiSession) findGeneralChannel(baseURL string) string {
	resp, err := s.client.Get(baseURL + "/api/v1/chat/channels")
	if err != nil {
		return ""
	}
	defer resp.Body.Close()
	var channels []struct {
		ID   string `json:"id"`
		Kind string `json:"kind"`
	}
	if json.NewDecoder(resp.Body).Decode(&channels) != nil {
		return ""
	}
	for _, c := range channels {
		if c.Kind == "general" {
			return c.ID
		}
	}
	return ""
}

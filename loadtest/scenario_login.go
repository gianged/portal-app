package main

import (
	"flag"
	"fmt"
	"net/http"
	"sync"
	"sync/atomic"
	"time"
)

// Morning login storm: the whole company logs in within a short window.
// Validates the NAT-safe auth rate limits and Argon2 cost under burst.
func runLoginStorm(args []string) error {
	fs := flag.NewFlagSet("login-storm", flag.ExitOnError)
	baseURL := fs.String("base-url", defaultBaseURL, "server base URL")
	usersFile := fs.String("users", "users.json", "seeded emails file")
	password := fs.String("password", "admin123", "shared seed password")
	rate := fs.Int("rate", 200, "logins per minute")
	duration := fs.Duration("duration", 5*time.Minute, "test duration")
	_ = fs.Parse(args)

	users, err := loadUsers(*usersFile)
	if err != nil {
		return err
	}
	client := &http.Client{Transport: newTransport(), Timeout: 10 * time.Second}

	var (
		hist   Hist
		ok     atomic.Int64
		failed atomic.Int64
		wg     sync.WaitGroup
	)
	fmt.Printf("login-storm: %d logins/min for %v against %s\n", *rate, *duration, *baseURL)

	interval := time.Minute / time.Duration(*rate)
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	deadline := time.Now().Add(*duration)
	for now := range ticker.C {
		if now.After(deadline) {
			break
		}
		wg.Add(1)
		go func() {
			defer wg.Done()
			start := time.Now()
			cookie, status, err := login(client, *baseURL, randomUser(users), *password)
			hist.Add(time.Since(start))
			if err == nil && status == http.StatusOK && cookie != "" {
				ok.Add(1)
			} else {
				failed.Add(1)
			}
		}()
	}
	wg.Wait()

	total := ok.Load() + failed.Load()
	fmt.Printf("logins: %d ok, %d failed | %s\n", ok.Load(), failed.Load(), hist.Summary())

	var t thresholds
	t.require(hist.Percentile(95) < time.Second, "p95 < 1s (got %v)", hist.Percentile(95))
	t.require(float64(failed.Load()) < 0.01*float64(total), "failure rate < 1%% (got %d/%d)", failed.Load(), total)
	return t.Err()
}

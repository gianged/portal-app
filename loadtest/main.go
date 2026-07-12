// Load-test harness for the portal, modelling 1000 employees at peak.
// One binary, three load scenarios plus a boot-resilience e2e:
//
//	go run . login-storm -rate 200 -duration 5m
//	go run . api-mix    -peak-rps 200
//	go run . ws-chat    -sockets 1000 -stagger 120ms -hold 2m
//	go run . boot-resilience
//
// Prerequisites: seeded DB and users.json (see README.md). The server must run
// with COOKIE_SECURE=false for plain-HTTP cookies. boot-resilience instead
// needs prebuilt binaries and drives docker compose itself.
package main

import (
	"encoding/json"
	"fmt"
	"math/rand"
	"net/http"
	"os"
	"strings"
	"time"
)

const defaultBaseURL = "http://127.0.0.1:8090"

// Fixed bootstrap id of the company-wide general channel.
const defaultGeneralID = "00000000-0000-7000-8000-0000000000a1"

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	cmd, args := os.Args[1], os.Args[2:]
	var err error
	switch cmd {
	case "login-storm":
		err = runLoginStorm(args)
	case "api-mix":
		err = runAPIMix(args)
	case "ws-chat":
		err = runWsChat(args)
	case "boot-resilience":
		err = runBootResilience(args)
	default:
		usage()
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "FAIL:", err)
		os.Exit(1)
	}
	fmt.Println("PASS")
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: loadtest <login-storm|api-mix|ws-chat|boot-resilience> [flags]")
	os.Exit(2)
}

// loadUsers reads the seeded account emails exported to users.json.
func loadUsers(path string) ([]string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("reading %s (see README.md for the export step): %w", path, err)
	}
	var users []string
	if err := json.Unmarshal(raw, &users); err != nil {
		return nil, fmt.Errorf("parsing %s: %w", path, err)
	}
	if len(users) == 0 {
		return nil, fmt.Errorf("%s is empty", path)
	}
	return users, nil
}

func randomUser(users []string) string {
	return users[rand.Intn(len(users))]
}

// login posts credentials and returns the session cookie value.
func login(client *http.Client, baseURL, email, password string) (string, int, error) {
	body := fmt.Sprintf(`{"email":%q,"password":%q}`, email, password)
	resp, err := client.Post(baseURL+"/api/v1/login", "application/json", strings.NewReader(body))
	if err != nil {
		return "", 0, err
	}
	defer resp.Body.Close()
	for _, c := range resp.Cookies() {
		if c.Name == "portal_session" {
			return c.Value, resp.StatusCode, nil
		}
	}
	return "", resp.StatusCode, nil
}

// transport shared by every scenario client: generous keep-alive pool so
// connection churn does not pollute latency numbers.
func newTransport() *http.Transport {
	return &http.Transport{
		MaxIdleConns:        2048,
		MaxIdleConnsPerHost: 2048,
		IdleConnTimeout:     90 * time.Second,
	}
}

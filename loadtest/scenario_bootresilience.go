package main

import (
	"flag"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"
)

// Boot-resilience e2e: with Postgres and Scylla stopped, server and workers
// must stay alive and log retries instead of exiting; once infra returns they
// must finish boot. A short STARTUP_TIMEOUT_SECS must end in a non-zero exit.
//
// Needs docker compose and prebuilt binaries (`cargo build -p server -p workers`).
func runBootResilience(args []string) error {
	fs := flag.NewFlagSet("boot-resilience", flag.ExitOnError)
	repoRoot := fs.String("repo-root", "..", "repo root containing .env and infra/")
	serverBin := fs.String("server-bin", "", "server binary (default <repo-root>/target/debug/server)")
	workersBin := fs.String("workers-bin", "", "workers binary (default <repo-root>/target/debug/workers)")
	baseURL := fs.String("base-url", defaultBaseURL, "server base URL")
	workersGrpc := fs.String("workers-grpc", "127.0.0.1:50052", "workers gRPC ingest address")
	outage := fs.Duration("outage", 15*time.Second, "how long infra stays down after the binaries start")
	_ = fs.Parse(args)

	root, err := filepath.Abs(*repoRoot)
	if err != nil {
		return err
	}
	server := orDefaultBin(*serverBin, root, "server")
	workers := orDefaultBin(*workersBin, root, "workers")
	for _, bin := range []string{server, workers} {
		if _, err := os.Stat(bin); err != nil {
			return fmt.Errorf("missing %s (run `cargo build -p server -p workers` first)", bin)
		}
	}

	// The asserted ports must be free, or a stale instance would fake a pass.
	client := &http.Client{Timeout: 2 * time.Second}
	if resp, err := client.Get(*baseURL + "/healthz"); err == nil {
		resp.Body.Close()
		return fmt.Errorf("%s already answers /healthz; stop the running server first", *baseURL)
	}
	if conn, err := net.DialTimeout("tcp", *workersGrpc, time.Second); err == nil {
		conn.Close()
		return fmt.Errorf("%s already accepts connections; stop the running workers first", *workersGrpc)
	}

	fmt.Println("boot-resilience: bringing infra up, then stopping postgres + scylla")
	if err := compose(root, "up", "-d", "--wait"); err != nil {
		return fmt.Errorf("infra up: %w", err)
	}
	// Whatever happens below, leave the dependency stack running.
	defer func() {
		fmt.Println("boot-resilience: restoring infra")
		_ = compose(root, "up", "-d", "--wait")
	}()
	if err := compose(root, "stop", "postgres", "scylla"); err != nil {
		return fmt.Errorf("stopping postgres/scylla: %w", err)
	}

	// Phase 1: boot with infra down; both binaries must survive the outage.
	fmt.Printf("boot-resilience: starting binaries; postgres + scylla stay down for %v\n", *outage)
	env := []string{"STARTUP_TIMEOUT_SECS=180"}
	srv, err := startProc("server", server, root, env)
	if err != nil {
		return err
	}
	defer srv.kill()
	wrk, err := startProc("workers", workers, root, env)
	if err != nil {
		return err
	}
	defer wrk.kill()

	outageEnd := time.Now().Add(*outage)
	for time.Now().Before(outageEnd) {
		for _, p := range []*proc{srv, wrk} {
			if exited, exitErr := p.exited(); exited {
				p.tail()
				return fmt.Errorf("%s exited during the outage (want: keep retrying): %v", p.name, exitErr)
			}
		}
		time.Sleep(time.Second)
	}
	for _, p := range []*proc{srv, wrk} {
		if !p.logContains("retrying") {
			p.tail()
			return fmt.Errorf("%s log has no retry warnings (see %s)", p.name, p.logPath)
		}
	}
	fmt.Println("boot-resilience: both binaries alive and retrying - ok")

	// Phase 2: infra returns; both binaries must finish boot within the budget.
	if err := compose(root, "up", "-d", "--wait", "postgres", "scylla"); err != nil {
		return fmt.Errorf("restarting postgres/scylla: %w", err)
	}
	deadline := time.Now().Add(3 * time.Minute)
	if err := waitHTTP(client, *baseURL+"/healthz", deadline, srv); err != nil {
		return err
	}
	if err := waitHTTP(client, *baseURL+"/readyz", deadline, srv); err != nil {
		return err
	}
	if err := waitTCP(*workersGrpc, deadline, wrk); err != nil {
		return err
	}
	fmt.Println("boot-resilience: server ready and workers serving after infra returned - ok")
	srv.kill()
	wrk.kill()

	// Phase 3: an expired budget must end in a clean non-zero exit.
	if err := compose(root, "stop", "postgres"); err != nil {
		return fmt.Errorf("stopping postgres: %w", err)
	}
	fmt.Println("boot-resilience: starting server with STARTUP_TIMEOUT_SECS=8; expecting give-up")
	doomed, err := startProc("server-giveup", server, root, []string{"STARTUP_TIMEOUT_SECS=8"})
	if err != nil {
		return err
	}
	defer doomed.kill()
	giveUp := time.Now().Add(time.Minute)
	for {
		if exited, exitErr := doomed.exited(); exited {
			if exitErr == nil {
				doomed.tail()
				return fmt.Errorf("give-up run exited zero (want non-zero)")
			}
			break
		}
		if time.Now().After(giveUp) {
			doomed.tail()
			return fmt.Errorf("server still running 60s after an 8s startup budget")
		}
		time.Sleep(time.Second)
	}
	fmt.Println("boot-resilience: bounded give-up with non-zero exit - ok")
	return nil
}

func orDefaultBin(explicit, root, name string) string {
	if explicit != "" {
		return explicit
	}
	bin := filepath.Join(root, "target", "debug", name)
	if runtime.GOOS == "windows" {
		bin += ".exe"
	}
	return bin
}

// compose runs docker compose against the dependency stack with the repo .env.
func compose(root string, args ...string) error {
	full := append([]string{"compose", "--env-file", ".env", "-f", "infra/docker-compose.infra.yml"}, args...)
	cmd := exec.Command("docker", full...)
	cmd.Dir = root
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

// proc is a spawned portal binary with its output captured to a log file.
type proc struct {
	name    string
	logPath string
	cmd     *exec.Cmd
	log     *os.File
	mu      sync.Mutex
	done    bool
	waitErr error
}

// startProc launches bin with dir as working directory (so the binary picks up
// the repo-root .env) and extraEnv overriding inherited variables.
func startProc(name, bin, dir string, extraEnv []string) (*proc, error) {
	logPath := "boot-" + name + ".log"
	logFile, err := os.Create(logPath)
	if err != nil {
		return nil, err
	}
	cmd := exec.Command(bin)
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), extraEnv...)
	cmd.Stdout = logFile
	cmd.Stderr = logFile
	if err := cmd.Start(); err != nil {
		logFile.Close()
		return nil, fmt.Errorf("starting %s: %w", name, err)
	}
	fmt.Printf("  started %s (pid %d, log %s)\n", name, cmd.Process.Pid, logPath)
	p := &proc{name: name, logPath: logPath, cmd: cmd, log: logFile}
	go func() {
		waitErr := cmd.Wait()
		p.mu.Lock()
		p.done, p.waitErr = true, waitErr
		p.mu.Unlock()
	}()
	return p, nil
}

func (p *proc) exited() (bool, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.done, p.waitErr
}

// kill terminates the process (no-op if already exited) and closes the log.
func (p *proc) kill() {
	_ = p.cmd.Process.Kill()
	for i := 0; i < 100; i++ {
		if done, _ := p.exited(); done {
			break
		}
		time.Sleep(100 * time.Millisecond)
	}
	p.log.Close()
}

func (p *proc) logContains(substr string) bool {
	raw, err := os.ReadFile(p.logPath)
	return err == nil && strings.Contains(string(raw), substr)
}

// tail prints the last log lines to help diagnose a failed phase.
func (p *proc) tail() {
	raw, err := os.ReadFile(p.logPath)
	if err != nil {
		return
	}
	lines := strings.Split(strings.TrimRight(string(raw), "\n"), "\n")
	if len(lines) > 15 {
		lines = lines[len(lines)-15:]
	}
	fmt.Printf("--- %s log tail ---\n%s\n", p.name, strings.Join(lines, "\n"))
}

// waitHTTP polls url until it returns 200, the deadline passes, or p exits.
func waitHTTP(client *http.Client, url string, deadline time.Time, p *proc) error {
	start := time.Now()
	for {
		if done, exitErr := p.exited(); done {
			p.tail()
			return fmt.Errorf("%s exited while waiting for %s: %v", p.name, url, exitErr)
		}
		resp, err := client.Get(url)
		if err == nil {
			resp.Body.Close()
			if resp.StatusCode == http.StatusOK {
				fmt.Printf("  %s -> 200 after %v\n", url, time.Since(start).Round(time.Second))
				return nil
			}
		}
		if time.Now().After(deadline) {
			p.tail()
			return fmt.Errorf("%s not answering 200 before deadline", url)
		}
		time.Sleep(time.Second)
	}
}

// waitTCP polls addr until it accepts, the deadline passes, or p exits.
func waitTCP(addr string, deadline time.Time, p *proc) error {
	start := time.Now()
	for {
		if done, exitErr := p.exited(); done {
			p.tail()
			return fmt.Errorf("%s exited while waiting for %s: %v", p.name, addr, exitErr)
		}
		conn, err := net.DialTimeout("tcp", addr, time.Second)
		if err == nil {
			conn.Close()
			fmt.Printf("  %s accepting after %v\n", addr, time.Since(start).Round(time.Second))
			return nil
		}
		if time.Now().After(deadline) {
			p.tail()
			return fmt.Errorf("%s not accepting before deadline", addr)
		}
		time.Sleep(time.Second)
	}
}

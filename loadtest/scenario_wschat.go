package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/gorilla/websocket"
)

// Live chat at scale: N concurrent WebSocket connections all subscribed to the
// general channel; a few senders chat. Every message fans out to every
// connection, so received-frame throughput is the real measure.
//
// Connects are staggered so the wave models a real morning ramp instead of a
// single-second stampede tripping AUTH_IP_RATE_LIMIT. Posting to general is
// HR-only (domain rule), so sender sockets log in with the HR account; keep
// senders x (60s/send-every) under CHAT_RATE_LIMIT (120/min, one account).
func runWsChat(args []string) error {
	fs := flag.NewFlagSet("ws-chat", flag.ExitOnError)
	baseURL := fs.String("base-url", defaultBaseURL, "server base URL")
	usersFile := fs.String("users", "users.json", "seeded emails file")
	password := fs.String("password", "admin123", "shared seed password")
	sockets := fs.Int("sockets", 200, "concurrent WebSocket connections")
	stagger := fs.Duration("stagger", 120*time.Millisecond, "per-socket connect delay")
	hold := fs.Duration("hold", 2*time.Minute, "how long each socket stays open")
	senderEvery := fs.Int("sender-every", 100, "every Nth socket is a sender")
	sendEvery := fs.Duration("send-every", 6*time.Second, "interval between a sender's messages")
	senderEmail := fs.String("sender-email", "hr@portal.local", "account allowed to post to general")
	generalID := fs.String("general-id", defaultGeneralID, "general channel id")
	_ = fs.Parse(args)

	users, err := loadUsers(*usersFile)
	if err != nil {
		return err
	}
	transport := newTransport()

	var (
		loginFail   atomic.Int64
		connectFail atomic.Int64
		connected   atomic.Int64
		subscribed  atomic.Int64
		sent        atomic.Int64
		received    atomic.Int64
		wsErrors    atomic.Int64
		errorLogged atomic.Int64
		frameLogged atomic.Int64
		connectHist Hist
		wg          sync.WaitGroup
	)
	fmt.Printf("ws-chat: %d sockets staggered %v apart, hold %v, %s\n", *sockets, *stagger, *hold, *baseURL)

	wsURL := strings.Replace(*baseURL, "http", "ws", 1) + "/api/v1/chat/ws"
	for i := 1; i <= *sockets; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			time.Sleep(time.Duration(i) * *stagger)

			isSender := i%*senderEvery == 0
			email := users[i%len(users)]
			if isSender {
				email = *senderEmail
			}
			client := &http.Client{Transport: transport, Timeout: 10 * time.Second}
			cookie, status, err := login(client, *baseURL, email, *password)
			if err != nil || status != http.StatusOK || cookie == "" {
				if loginFail.Add(1) <= 3 {
					fmt.Printf("  socket %d login failed: status=%d err=%v email=%s\n", i, status, err, email)
				}
				return
			}

			start := time.Now()
			header := http.Header{"Cookie": {"portal_session=" + cookie}}
			conn, _, err := websocket.DefaultDialer.Dial(wsURL, header)
			connectHist.Add(time.Since(start))
			if err != nil {
				connectFail.Add(1)
				return
			}
			defer conn.Close()
			connected.Add(1)

			// gorilla allows one concurrent writer; serialize all sends.
			var writeMu sync.Mutex
			send := func(frame string) bool {
				writeMu.Lock()
				defer writeMu.Unlock()
				return conn.WriteMessage(websocket.TextMessage, []byte(frame)) == nil
			}

			send(fmt.Sprintf(`{"type":"subscribe","channel_id":%q}`, *generalID))

			done := make(chan struct{})
			go func() {
				defer close(done)
				for {
					_, raw, err := conn.ReadMessage()
					if err != nil {
						return
					}
					// RawMessage: "message" is an object on message_created
					// frames but a string on error frames.
					var frame struct {
						Type    string          `json:"type"`
						Code    string          `json:"code"`
						Message json.RawMessage `json:"message"`
					}
					if err := json.Unmarshal(raw, &frame); err != nil {
						wsErrors.Add(1)
						if errorLogged.Add(1) <= 5 {
							fmt.Printf("  socket %d bad frame (%v): %.120s\n", i, err, raw)
						}
						continue
					}
					if frameLogged.Add(1) <= 5 {
						fmt.Printf("  socket %d frame: %.120s\n", i, raw)
					}
					switch frame.Type {
					case "subscribed":
						subscribed.Add(1)
					case "message_created":
						received.Add(1)
					case "error":
						wsErrors.Add(1)
						if errorLogged.Add(1) <= 5 {
							fmt.Printf("  socket %d error frame: %s %s\n", i, frame.Code, frame.Message)
						}
					}
				}
			}()

			// Client-side keepalive well inside the server's 30s heartbeat.
			ping := time.NewTicker(20 * time.Second)
			defer ping.Stop()
			var chat *time.Ticker
			var chatC <-chan time.Time
			if isSender {
				chat = time.NewTicker(*sendEvery)
				chatC = chat.C
				defer chat.Stop()
			}
			deadline := time.After(*hold)
			for {
				select {
				case <-ping.C:
					send(`{"type":"ping"}`)
				case <-chatC:
					body := fmt.Sprintf("load test message from socket %d at %d", i, time.Now().UnixMilli())
					if send(fmt.Sprintf(`{"type":"send_message","channel_id":%q,"body":%q,"mentions":[],"attachment_keys":[]}`, *generalID, body)) {
						sent.Add(1)
					}
				case <-deadline:
					_ = conn.WriteMessage(websocket.CloseMessage,
						websocket.FormatCloseMessage(websocket.CloseNormalClosure, ""))
					conn.Close()
					<-done
					return
				case <-done:
					// Server closed on us before the hold elapsed.
					wsErrors.Add(1)
					return
				}
			}
		}(i)
	}
	wg.Wait()

	fmt.Printf("sockets: %d connected (%d subscribed), %d login-fail, %d connect-fail | connect %s\n",
		connected.Load(), subscribed.Load(), loginFail.Load(), connectFail.Load(), connectHist.Summary())
	fmt.Printf("chat: %d sent, %d received (fan-out frames), %d ws errors\n",
		sent.Load(), received.Load(), wsErrors.Load())

	var t thresholds
	fails := loginFail.Load() + connectFail.Load()
	t.require(float64(fails) < 0.01*float64(*sockets), "login+connect failures < 1%% (got %d/%d)",
		fails, *sockets)
	t.require(wsErrors.Load() < 10, "ws errors < 10 (got %d)", wsErrors.Load())
	t.require(received.Load() > 0, "fan-out frames received (got %d)", received.Load())
	return t.Err()
}

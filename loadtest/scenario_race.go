// race: concurrent-writer integrity checks against a live portal stack.
//
// Every scenario points several authenticated sessions at the same entity,
// releases them on a barrier, and asserts what a correct implementation must
// guarantee under concurrent modification:
//
//   - a lifecycle transition has exactly one winner; losers get a clean 409
//   - DB-constraint backstops surface as 409, never as 500
//   - concurrent single-field PATCHes do not clobber each other's fields
//   - the entity's final state matches the winning writer
//
//	go run . race [-writers 6] [-rounds 8]
//
// Prerequisites: running stack with the demo seed applied (hr@portal.local)
// and COOKIE_SECURE=false. The defaults keep every actor under the per-user
// API rate limit (120 req / 60 s); raise API_RATE_LIMIT before raising
// -writers or -rounds.

package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"net/http/cookiejar"
	"strings"
	"sync"
	"time"
)

const racePassword = "race-pass-12345"

func runRace(args []string) error {
	fs := flag.NewFlagSet("race", flag.ExitOnError)
	baseURL := fs.String("base-url", defaultBaseURL, "server base URL")
	hrEmail := fs.String("hr-email", "hr@portal.local", "seeded HR account used for provisioning")
	password := fs.String("password", "admin123", "password of the seeded HR account")
	writers := fs.Int("writers", 6, "concurrent writers per burst")
	rounds := fs.Int("rounds", 8, "rounds per scenario")
	if err := fs.Parse(args); err != nil {
		return err
	}

	fmt.Printf("race: base=%s writers=%d rounds=%d\n", *baseURL, *writers, *rounds)
	w, err := setupRaceWorld(*baseURL, *hrEmail, *password)
	if err != nil {
		return err
	}

	th := &thresholds{}
	raceRequestSubmit(w, th, *writers, *rounds)
	raceRequestAssign(w, th, *writers, *rounds)
	raceRequestReview(w, th, *writers, *rounds)
	raceRequestPatch(w, th, *rounds)
	raceTicket(w, th, *writers, *rounds)
	projectRounds := max(2, *rounds/2)
	raceProjectStatus(w, th, *writers, projectRounds)
	raceProjectResume(w, th, *writers, projectRounds)
	raceGroupLeader(w, th, *rounds)
	raceUserDeactivate(w, th, *writers, projectRounds)
	return th.Err()
}

// --- authenticated session ---

type raceSession struct {
	base   string
	email  string
	userID string
	client *http.Client
}

func newRaceSession(base, email, password string) (*raceSession, error) {
	jar, err := cookiejar.New(nil)
	if err != nil {
		return nil, err
	}
	s := &raceSession{
		base:   base,
		email:  email,
		client: &http.Client{Transport: newTransport(), Jar: jar, Timeout: 15 * time.Second},
	}
	status, body, err := s.do(http.MethodPost, "/api/v1/login",
		jsonBody(map[string]any{"email": email, "password": password}))
	if err != nil {
		return nil, fmt.Errorf("login %s: %w", email, err)
	}
	if status != http.StatusOK {
		return nil, fmt.Errorf("login %s: status %d: %s", email, status, trimBody(body))
	}
	var out struct {
		User struct {
			ID string `json:"id"`
		} `json:"user"`
	}
	if err := json.Unmarshal([]byte(body), &out); err != nil || out.User.ID == "" {
		return nil, fmt.Errorf("login %s: cannot decode user id from %s", email, trimBody(body))
	}
	s.userID = out.User.ID
	return s, nil
}

func (s *raceSession) do(method, path, body string) (int, string, error) {
	var reader io.Reader
	if body != "" {
		reader = strings.NewReader(body)
	}
	req, err := http.NewRequest(method, s.base+path, reader)
	if err != nil {
		return 0, "", err
	}
	if body != "" {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := s.client.Do(req)
	if err != nil {
		return 0, "", err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return resp.StatusCode, "", err
	}
	return resp.StatusCode, string(raw), nil
}

// mustDo runs one setup step and errors unless it returned 2xx.
func (s *raceSession) mustDo(method, path, body, what string) (string, error) {
	status, respBody, err := s.do(method, path, body)
	if err != nil {
		return "", fmt.Errorf("%s: %w", what, err)
	}
	if status < 200 || status > 299 {
		return "", fmt.Errorf("%s: status %d: %s", what, status, trimBody(respBody))
	}
	return respBody, nil
}

func jsonBody(v any) string {
	raw, _ := json.Marshal(v)
	return string(raw)
}

func idOf(body string) string {
	var out struct {
		ID string `json:"id"`
	}
	if json.Unmarshal([]byte(body), &out) != nil {
		return ""
	}
	return out.ID
}

func trimBody(s string) string {
	s = strings.ReplaceAll(s, "\n", " ")
	if len(s) > 160 {
		return s[:160] + "..."
	}
	return s
}

// --- world provisioning ---

// raceWorld is the cast assembled once per run: enough distinct actors that
// each scenario stays inside its own per-user rate-limit budget.
type raceWorld struct {
	base         string
	runID        string
	hr           *raceSession // seeded HR, provisioning only
	hr2          *raceSession // dedicated HR for group/user scenarios
	leader       *raceSession // leader of the race group
	members      []*raceSession
	subleaders   []*raceSession // sub-leaders of the race group; can assign/approve
	it           []*raceSession
	candidateIDs [2]string // leaderless-group promotion targets, never log in
	victimID     string    // deactivation target; logs in once to leave pending
	groupID      string
	projectID    string
}

func setupRaceWorld(base, hrEmail, password string) (*raceWorld, error) {
	hr, err := newRaceSession(base, hrEmail, password)
	if err != nil {
		return nil, fmt.Errorf(
			"HR login failed (stack up? demo seed applied? COOKIE_SECURE=false?): %w", err)
	}
	w := &raceWorld{base: base, hr: hr, runID: fmt.Sprintf("%x", time.Now().UnixNano())}

	if w.hr2, _, err = w.provisionUser("hr", 0, "hr", true); err != nil {
		return nil, err
	}
	if w.leader, _, err = w.provisionUser("leader", 0, "", true); err != nil {
		return nil, err
	}
	for i := 0; i < 4; i++ {
		m, _, err := w.provisionUser("member", i, "", true)
		if err != nil {
			return nil, err
		}
		w.members = append(w.members, m)
	}
	for i := 0; i < 4; i++ {
		s, _, err := w.provisionUser("sublead", i, "", true)
		if err != nil {
			return nil, err
		}
		w.subleaders = append(w.subleaders, s)
	}
	for i := 0; i < 3; i++ {
		u, _, err := w.provisionUser("it", i, "", true)
		if err != nil {
			return nil, err
		}
		w.it = append(w.it, u)
	}
	for i := range w.candidateIDs {
		_, id, err := w.provisionUser("cand", i, "", false)
		if err != nil {
			return nil, err
		}
		w.candidateIDs[i] = id
	}
	// the victim logs in once so first-login promotes it out of pending
	victim, _, err := w.provisionUser("victim", 0, "", true)
	if err != nil {
		return nil, err
	}
	w.victimID = victim.userID

	resp, err := w.hr.mustDo(http.MethodPost, "/api/v1/groups", jsonBody(map[string]any{
		"name":        "Race Group " + w.runID,
		"description": "concurrent writer probe",
		"kind":        "standard",
	}), "create race group")
	if err != nil {
		return nil, err
	}
	w.groupID = idOf(resp)
	if err := w.addMember(w.groupID, w.leader.userID, "leader"); err != nil {
		return nil, err
	}
	for _, m := range w.members {
		if err := w.addMember(w.groupID, m.userID, "member"); err != nil {
			return nil, err
		}
	}
	for _, s := range w.subleaders {
		if err := w.addMember(w.groupID, s.userID, "sub_leader"); err != nil {
			return nil, err
		}
	}

	itGroupID, err := w.ensureITGroup()
	if err != nil {
		return nil, err
	}
	for _, u := range w.it {
		if err := w.addMember(itGroupID, u.userID, "member"); err != nil {
			return nil, err
		}
	}

	if w.projectID, err = w.newActiveProject("Race Project " + w.runID); err != nil {
		return nil, err
	}
	fmt.Printf("race: world ready (group=%s project=%s)\n", w.groupID, w.projectID)
	return w, nil
}

func (w *raceWorld) provisionUser(kind string, i int, systemRole string, withSession bool) (*raceSession, string, error) {
	email := fmt.Sprintf("race-%s%d-%s@portal.local", kind, i, w.runID)
	body := map[string]any{
		"email":     email,
		"password":  racePassword,
		"full_name": fmt.Sprintf("Race %s %d", kind, i),
		"phone":     nil,
		"timezone":  "UTC",
	}
	if systemRole != "" {
		body["system_role"] = systemRole
	}
	resp, err := w.hr.mustDo(http.MethodPost, "/api/v1/users", jsonBody(body), "create user "+email)
	if err != nil {
		return nil, "", err
	}
	id := idOf(resp)
	if id == "" {
		return nil, "", fmt.Errorf("create user %s: no id in response %s", email, trimBody(resp))
	}
	if !withSession {
		return nil, id, nil
	}
	sess, err := newRaceSession(w.base, email, racePassword)
	return sess, id, err
}

func (w *raceWorld) addMember(groupID, userID, role string) error {
	_, err := w.hr.mustDo(http.MethodPost, "/api/v1/groups/"+groupID+"/members",
		jsonBody(map[string]any{"user_id": userID, "role": role}),
		"add member "+userID+" to "+groupID)
	return err
}

// ensureITGroup returns the single IT group, creating it on an unseeded stack.
func (w *raceWorld) ensureITGroup() (string, error) {
	resp, err := w.hr.mustDo(http.MethodGet, "/api/v1/groups", "", "list groups")
	if err != nil {
		return "", err
	}
	var groups []struct {
		ID   string `json:"id"`
		Kind string `json:"kind"`
	}
	if err := json.Unmarshal([]byte(resp), &groups); err != nil {
		return "", fmt.Errorf("decode group list: %w", err)
	}
	for _, g := range groups {
		if g.Kind == "it" {
			return g.ID, nil
		}
	}
	resp, err = w.hr.mustDo(http.MethodPost, "/api/v1/groups", jsonBody(map[string]any{
		"name":        "Race IT " + w.runID,
		"description": "concurrent writer probe",
		"kind":        "it",
	}), "create it group")
	if err != nil {
		return "", err
	}
	return idOf(resp), nil
}

func (w *raceWorld) newActiveProject(name string) (string, error) {
	resp, err := w.leader.mustDo(http.MethodPost, "/api/v1/projects", jsonBody(map[string]any{
		"owner_group_id": w.groupID,
		"name":           name,
		"description":    "concurrent writer probe",
	}), "create project")
	if err != nil {
		return "", err
	}
	id := idOf(resp)
	if _, err := w.leader.mustDo(http.MethodPost, "/api/v1/projects/"+id+"/status",
		`{"status":"active"}`, "activate project"); err != nil {
		return "", err
	}
	return id, nil
}

func (w *raceWorld) newDraftRequest(creator *raceSession, tag string) (string, error) {
	resp, err := creator.mustDo(http.MethodPost, "/api/v1/requests", jsonBody(map[string]any{
		"project_id":  w.projectID,
		"title":       "Race request " + tag,
		"description": "concurrent writer probe",
		"priority":    "normal",
	}), "create request")
	if err != nil {
		return "", err
	}
	return idOf(resp), nil
}

// --- burst engine ---

type shot struct {
	sess   *raceSession
	method string
	path   string
	body   string
	tag    string // writer's intent, e.g. target status or assignee id
}

type shotResult struct {
	shot
	status int
	body   string
	err    error
}

// fire releases every shot at the same instant and waits for all responses.
func fire(shots []shot) []shotResult {
	results := make([]shotResult, len(shots))
	start := make(chan struct{})
	var wg sync.WaitGroup
	for i := range shots {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			<-start
			status, body, err := shots[i].sess.do(shots[i].method, shots[i].path, shots[i].body)
			results[i] = shotResult{shot: shots[i], status: status, body: body, err: err}
		}(i)
	}
	// let every goroutine park on the barrier before release
	time.Sleep(50 * time.Millisecond)
	close(start)
	wg.Wait()
	return results
}

type outcome struct {
	wins, conflicts, limited, serverErr, other, failed []shotResult
}

func classify(results []shotResult) outcome {
	var o outcome
	for _, r := range results {
		switch {
		case r.err != nil:
			o.failed = append(o.failed, r)
		case r.status >= 200 && r.status < 300:
			o.wins = append(o.wins, r)
		case r.status == http.StatusConflict:
			o.conflicts = append(o.conflicts, r)
		case r.status == http.StatusTooManyRequests:
			o.limited = append(o.limited, r)
		case r.status >= 500:
			o.serverErr = append(o.serverErr, r)
		default:
			o.other = append(o.other, r)
		}
	}
	return o
}

// --- per-scenario accounting ---

type raceStats struct {
	name       string
	rounds     int
	setupFails int
	transport  int
	limited    int
	serverErr  int
	other      int
	multiWin   int
	zeroWin    int
	refused    int
	mismatch   int
}

func newStats(name string) *raceStats {
	return &raceStats{name: name}
}

func (st *raceStats) notef(format string, args ...any) {
	fmt.Printf("  [%s] %s\n", st.name, fmt.Sprintf(format, args...))
}

func (st *raceStats) setupFailf(round int, err error) {
	st.setupFails++
	st.notef("round %d: setup failed: %v", round, err)
}

func (st *raceStats) mismatchf(format string, args ...any) {
	st.mismatch++
	st.notef(format, args...)
}

// foldErrors absorbs the non-contract buckets shared by every burst kind.
func (st *raceStats) foldErrors(round int, o outcome) {
	if n := len(o.failed); n > 0 {
		st.transport += n
		st.notef("round %d: %d transport errors (first: %v)", round, n, o.failed[0].err)
	}
	st.limited += len(o.limited)
	if n := len(o.serverErr); n > 0 {
		st.serverErr += n
		st.notef("round %d: %d x 5xx on %s (first: %d %s)",
			round, n, o.serverErr[0].path, o.serverErr[0].status, trimBody(o.serverErr[0].body))
	}
	if n := len(o.other); n > 0 {
		st.other += n
		st.notef("round %d: unexpected status %d on %s: %s",
			round, o.other[0].status, o.other[0].path, trimBody(o.other[0].body))
	}
}

// absorb applies the exactly-one-winner contract for conflicting transitions
// and returns the winner when the round produced exactly one.
func (st *raceStats) absorb(round int, o outcome) *shotResult {
	st.rounds++
	st.foldErrors(round, o)
	switch len(o.wins) {
	case 1:
		return &o.wins[0]
	case 0:
		// only broken when nothing else explains the missing winner
		if len(o.failed)+len(o.limited)+len(o.serverErr)+len(o.other) == 0 {
			st.zeroWin++
			st.notef("round %d: no winner; every writer was refused", round)
		}
	default:
		st.multiWin++
		tags := make([]string, len(o.wins))
		for i, r := range o.wins {
			tags[i] = r.tag
		}
		st.notef("round %d: %d winners on %s (%s) - lost update",
			round, len(o.wins), o.wins[0].path, strings.Join(tags, ", "))
	}
	return nil
}

// absorbAll applies the everyone-wins contract for compatible writes.
func (st *raceStats) absorbAll(round int, o outcome) bool {
	st.rounds++
	st.foldErrors(round, o)
	if n := len(o.conflicts); n > 0 {
		st.refused += n
		st.notef("round %d: %d compatible writers refused with 409", round, n)
	}
	return len(o.conflicts)+len(o.failed)+len(o.limited)+len(o.serverErr)+len(o.other) == 0
}

func (st *raceStats) report(th *thresholds) {
	clean := st.setupFails == 0 && st.transport == 0 && st.limited == 0 &&
		st.serverErr == 0 && st.other == 0 && st.multiWin == 0 &&
		st.zeroWin == 0 && st.refused == 0 && st.mismatch == 0
	if clean {
		th.require(true, "%s: %d rounds, concurrency contract held", st.name, st.rounds)
		return
	}
	if st.setupFails > 0 {
		th.require(false, "%s: %d rounds aborted during setup", st.name, st.setupFails)
	}
	if st.transport > 0 {
		th.require(false, "%s: %d transport errors", st.name, st.transport)
	}
	if st.limited > 0 {
		th.require(false, "%s: %d writers rate-limited (429); raise API_RATE_LIMIT or lower -writers/-rounds",
			st.name, st.limited)
	}
	if st.serverErr > 0 {
		th.require(false, "%s: %d writers got 5xx (concurrent write surfaced an internal error)",
			st.name, st.serverErr)
	}
	if st.other > 0 {
		th.require(false, "%s: %d writers got an unexpected status", st.name, st.other)
	}
	if st.multiWin > 0 {
		th.require(false, "%s: %d/%d rounds had multiple transition winners (lost update)",
			st.name, st.multiWin, st.rounds)
	}
	if st.zeroWin > 0 {
		th.require(false, "%s: %d/%d rounds ended with no winner", st.name, st.zeroWin, st.rounds)
	}
	if st.refused > 0 {
		th.require(false, "%s: %d compatible writers were refused", st.name, st.refused)
	}
	if st.mismatch > 0 {
		th.require(false, "%s: %d final-state checks failed", st.name, st.mismatch)
	}
}

// --- final-state readers ---

type reqState struct {
	Status      string
	Title       string
	Description string
	Priority    string
	AssigneeID  string
}

func (w *raceWorld) requestState(viewer *raceSession, id string) (reqState, bool) {
	resp, err := viewer.mustDo(http.MethodGet, "/api/v1/requests/"+id, "", "read request")
	if err != nil {
		return reqState{}, false
	}
	var out struct {
		Request struct {
			Status      string `json:"status"`
			Title       string `json:"title"`
			Description string `json:"description"`
			Priority    string `json:"priority"`
			Assignee    *struct {
				ID string `json:"id"`
			} `json:"assignee"`
		} `json:"request"`
	}
	if json.Unmarshal([]byte(resp), &out) != nil {
		return reqState{}, false
	}
	s := reqState{
		Status:      out.Request.Status,
		Title:       out.Request.Title,
		Description: out.Request.Description,
		Priority:    out.Request.Priority,
	}
	if out.Request.Assignee != nil {
		s.AssigneeID = out.Request.Assignee.ID
	}
	return s, true
}

type tickState struct {
	Status     string
	Priority   string
	AssigneeID string
}

func (w *raceWorld) ticketState(viewer *raceSession, id string) (tickState, bool) {
	resp, err := viewer.mustDo(http.MethodGet, "/api/v1/tickets/"+id, "", "read ticket")
	if err != nil {
		return tickState{}, false
	}
	var out struct {
		Status   string  `json:"status"`
		Priority *string `json:"priority"`
		Assignee *struct {
			ID string `json:"id"`
		} `json:"assignee"`
	}
	if json.Unmarshal([]byte(resp), &out) != nil {
		return tickState{}, false
	}
	s := tickState{Status: out.Status}
	if out.Priority != nil {
		s.Priority = *out.Priority
	}
	if out.Assignee != nil {
		s.AssigneeID = out.Assignee.ID
	}
	return s, true
}

func (w *raceWorld) projectStatus(viewer *raceSession, id string) (string, bool) {
	resp, err := viewer.mustDo(http.MethodGet, "/api/v1/projects/"+id, "", "read project")
	if err != nil {
		return "", false
	}
	var out struct {
		Project struct {
			Status string `json:"status"`
		} `json:"project"`
	}
	if json.Unmarshal([]byte(resp), &out) != nil {
		return "", false
	}
	return out.Project.Status, true
}

// activeLeaderIDs returns the user ids currently holding the leader role.
func (w *raceWorld) activeLeaderIDs(viewer *raceSession, groupID string) ([]string, bool) {
	resp, err := viewer.mustDo(http.MethodGet, "/api/v1/groups/"+groupID, "", "read group")
	if err != nil {
		return nil, false
	}
	var out struct {
		Members []struct {
			User struct {
				ID string `json:"id"`
			} `json:"user"`
			Role   string `json:"role"`
			Active bool   `json:"active"`
		} `json:"members"`
	}
	if json.Unmarshal([]byte(resp), &out) != nil {
		return nil, false
	}
	var leaders []string
	for _, m := range out.Members {
		if m.Active && m.Role == "leader" {
			leaders = append(leaders, m.User.ID)
		}
	}
	return leaders, true
}

func (w *raceWorld) userStatus(viewer *raceSession, id string) (string, bool) {
	resp, err := viewer.mustDo(http.MethodGet, "/api/v1/users/"+id, "", "read user")
	if err != nil {
		return "", false
	}
	var out struct {
		Status string `json:"status"`
	}
	if json.Unmarshal([]byte(resp), &out) != nil {
		return "", false
	}
	return out.Status, true
}

// --- scenarios ---

// raceRequestSubmit: N identical submits of the same draft. Draft -> Submitted
// is a one-way edge, so exactly one submit may succeed.
func raceRequestSubmit(w *raceWorld, th *thresholds, writers, rounds int) {
	st := newStats("request-submit")
	for r := 0; r < rounds; r++ {
		creator := w.members[r%len(w.members)]
		reqID, err := w.newDraftRequest(creator, fmt.Sprintf("submit-%d", r))
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		shots := make([]shot, writers)
		for i := range shots {
			shots[i] = shot{
				sess: creator, method: http.MethodPost,
				path: "/api/v1/requests/" + reqID + "/submit", tag: "submit",
			}
		}
		st.absorb(r, classify(fire(shots)))
		if s, ok := w.requestState(creator, reqID); ok && s.Status != "submitted" {
			st.mismatchf("round %d: final status %q, want submitted", r, s.Status)
		}
	}
	st.report(th)
}

// raceRequestAssign: several sub-leaders assign the same submitted request to
// different assignees. Submitted -> Assigned fires once; the stored assignee
// must be the winner's target.
func raceRequestAssign(w *raceWorld, th *thresholds, writers, rounds int) {
	st := newStats("request-assign")
	for r := 0; r < rounds; r++ {
		creator := w.members[r%len(w.members)]
		reqID, err := w.newDraftRequest(creator, fmt.Sprintf("assign-%d", r))
		if err == nil {
			_, err = creator.mustDo(http.MethodPost, "/api/v1/requests/"+reqID+"/submit", "", "submit request")
		}
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		shots := make([]shot, writers)
		for i := range shots {
			target := w.members[i%len(w.members)].userID
			shots[i] = shot{
				sess:   w.subleaders[i%len(w.subleaders)],
				method: http.MethodPost,
				path:   "/api/v1/requests/" + reqID + "/assign",
				body:   jsonBody(map[string]any{"assignee_user_id": target}),
				tag:    target,
			}
		}
		winner := st.absorb(r, classify(fire(shots)))
		s, ok := w.requestState(creator, reqID)
		if !ok {
			st.mismatchf("round %d: cannot read final state", r)
			continue
		}
		if s.Status != "assigned" {
			st.mismatchf("round %d: final status %q, want assigned", r, s.Status)
		}
		if winner != nil && s.AssigneeID != winner.tag {
			st.mismatchf("round %d: final assignee %s is not the winner's target %s (lost update)",
				r, s.AssigneeID, winner.tag)
		}
	}
	st.report(th)
}

// raceRequestReview: approve and reject race on the same in-review request.
// Exactly one decision may land and the final status must match it.
func raceRequestReview(w *raceWorld, th *thresholds, writers, rounds int) {
	st := newStats("request-approve-reject")
	for r := 0; r < rounds; r++ {
		creator := w.members[r%len(w.members)]
		assignee := w.members[(r+1)%len(w.members)]
		sublead := w.subleaders[r%len(w.subleaders)]
		reqID, err := w.newDraftRequest(creator, fmt.Sprintf("review-%d", r))
		if err == nil {
			_, err = creator.mustDo(http.MethodPost, "/api/v1/requests/"+reqID+"/submit", "", "submit request")
		}
		if err == nil {
			_, err = sublead.mustDo(http.MethodPost, "/api/v1/requests/"+reqID+"/assign",
				jsonBody(map[string]any{"assignee_user_id": assignee.userID}), "assign request")
		}
		if err == nil {
			_, err = assignee.mustDo(http.MethodPost, "/api/v1/requests/"+reqID+"/start", "", "start request")
		}
		if err == nil {
			_, err = assignee.mustDo(http.MethodPost, "/api/v1/requests/"+reqID+"/review", "", "send for review")
		}
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		wantByAction := map[string]string{"approve": "completed", "reject": "in_progress"}
		shots := make([]shot, writers)
		for i := range shots {
			action := "approve"
			if i%2 == 1 {
				action = "reject"
			}
			shots[i] = shot{
				sess:   w.subleaders[i%len(w.subleaders)],
				method: http.MethodPost,
				path:   "/api/v1/requests/" + reqID + "/" + action,
				tag:    action,
			}
		}
		winner := st.absorb(r, classify(fire(shots)))
		s, ok := w.requestState(creator, reqID)
		if !ok {
			st.mismatchf("round %d: cannot read final state", r)
			continue
		}
		if winner != nil && s.Status != wantByAction[winner.tag] {
			st.mismatchf("round %d: winner was %s but final status is %q",
				r, winner.tag, s.Status)
		}
		if s.Status != "completed" && s.Status != "in_progress" {
			st.mismatchf("round %d: final status %q is outside the decision set", r, s.Status)
		}
	}
	st.report(th)
}

// raceRequestPatch: three concurrent single-field PATCHes on the same draft.
// All are compatible, so all must succeed and every field must stick.
func raceRequestPatch(w *raceWorld, th *thresholds, rounds int) {
	st := newStats("request-patch-fields")
	for r := 0; r < rounds; r++ {
		creator := w.members[r%len(w.members)]
		reqID, err := w.newDraftRequest(creator, fmt.Sprintf("patch-%d", r))
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		title := fmt.Sprintf("Race title %d-%s", r, w.runID)
		description := fmt.Sprintf("race description %d-%s", r, w.runID)
		path := "/api/v1/requests/" + reqID
		shots := []shot{
			{sess: creator, method: http.MethodPatch, path: path,
				body: jsonBody(map[string]any{"title": title}), tag: "title"},
			{sess: creator, method: http.MethodPatch, path: path,
				body: jsonBody(map[string]any{"description": description}), tag: "description"},
			{sess: creator, method: http.MethodPatch, path: path,
				body: jsonBody(map[string]any{"priority": "urgent"}), tag: "priority"},
		}
		st.absorbAll(r, classify(fire(shots)))
		s, ok := w.requestState(creator, reqID)
		if !ok {
			st.mismatchf("round %d: cannot read final state", r)
			continue
		}
		if s.Title != title {
			st.mismatchf("round %d: title clobbered by a concurrent patch (got %q)", r, s.Title)
		}
		if s.Description != description {
			st.mismatchf("round %d: description clobbered by a concurrent patch (got %q)", r, s.Description)
		}
		if s.Priority != "urgent" {
			st.mismatchf("round %d: priority clobbered by a concurrent patch (got %q)", r, s.Priority)
		}
	}
	st.report(th)
}

// raceTicket: IT staff race the triage of a fresh ticket, then race assigning
// it to different IT assignees. Each edge fires once.
func raceTicket(w *raceWorld, th *thresholds, writers, rounds int) {
	stTriage := newStats("ticket-triage")
	stAssign := newStats("ticket-assign")
	priorities := []string{"low", "normal", "high", "urgent"}
	for r := 0; r < rounds; r++ {
		requester := w.members[r%len(w.members)]
		resp, err := requester.mustDo(http.MethodPost, "/api/v1/tickets", jsonBody(map[string]any{
			"title":       fmt.Sprintf("Race ticket %d", r),
			"description": "concurrent writer probe",
			"category":    "software",
		}), "raise ticket")
		if err != nil {
			stTriage.setupFailf(r, err)
			continue
		}
		ticketID := idOf(resp)

		shots := make([]shot, writers)
		for i := range shots {
			priority := priorities[i%len(priorities)]
			shots[i] = shot{
				sess:   w.it[i%len(w.it)],
				method: http.MethodPost,
				path:   "/api/v1/tickets/" + ticketID + "/triage",
				body:   jsonBody(map[string]any{"priority": priority}),
				tag:    priority,
			}
		}
		winner := stTriage.absorb(r, classify(fire(shots)))
		s, ok := w.ticketState(requester, ticketID)
		if !ok {
			stTriage.mismatchf("round %d: cannot read final state", r)
			continue
		}
		if s.Status != "triaged" {
			stTriage.mismatchf("round %d: final status %q, want triaged", r, s.Status)
			continue
		}
		if winner != nil && s.Priority != winner.tag {
			stTriage.mismatchf("round %d: final priority %q is not the winner's %q (lost update)",
				r, s.Priority, winner.tag)
		}

		shots = make([]shot, writers)
		for i := range shots {
			target := w.it[i%len(w.it)].userID
			shots[i] = shot{
				sess:   w.it[(i+1)%len(w.it)],
				method: http.MethodPost,
				path:   "/api/v1/tickets/" + ticketID + "/assign",
				body:   jsonBody(map[string]any{"assignee_user_id": target}),
				tag:    target,
			}
		}
		winner = stAssign.absorb(r, classify(fire(shots)))
		s, ok = w.ticketState(requester, ticketID)
		if !ok {
			stAssign.mismatchf("round %d: cannot read final state", r)
			continue
		}
		if s.Status != "assigned" {
			stAssign.mismatchf("round %d: final status %q, want assigned", r, s.Status)
		}
		if winner != nil && s.AssigneeID != winner.tag {
			stAssign.mismatchf("round %d: final assignee %s is not the winner's target %s (lost update)",
				r, s.AssigneeID, winner.tag)
		}
	}
	stTriage.report(th)
	stAssign.report(th)
}

// raceProjectStatus: conflicting terminal transitions (hold/complete/cancel)
// race on the same active project. Exactly one may land.
func raceProjectStatus(w *raceWorld, th *thresholds, writers, rounds int) {
	st := newStats("project-status")
	targets := []string{"on_hold", "completed", "cancelled"}
	for r := 0; r < rounds; r++ {
		projectID, err := w.newActiveProject(fmt.Sprintf("Race status %d-%s", r, w.runID))
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		shots := make([]shot, writers)
		for i := range shots {
			target := targets[i%len(targets)]
			shots[i] = shot{
				sess:   w.leader,
				method: http.MethodPost,
				path:   "/api/v1/projects/" + projectID + "/status",
				body:   jsonBody(map[string]any{"status": target}),
				tag:    target,
			}
		}
		winner := st.absorb(r, classify(fire(shots)))
		status, ok := w.projectStatus(w.leader, projectID)
		if !ok {
			st.mismatchf("round %d: cannot read final state", r)
			continue
		}
		if winner != nil && status != winner.tag {
			st.mismatchf("round %d: winner set %q but final status is %q", r, winner.tag, status)
		}
	}
	st.report(th)
}

// raceProjectResume: N identical resumes of an on-hold project. The handler
// picks activate-vs-resume from the state it read, so this hammers that
// read-then-decide window. One resume may win.
func raceProjectResume(w *raceWorld, th *thresholds, writers, rounds int) {
	st := newStats("project-resume")
	for r := 0; r < rounds; r++ {
		projectID, err := w.newActiveProject(fmt.Sprintf("Race resume %d-%s", r, w.runID))
		if err == nil {
			_, err = w.leader.mustDo(http.MethodPost, "/api/v1/projects/"+projectID+"/status",
				`{"status":"on_hold"}`, "hold project")
		}
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		shots := make([]shot, writers)
		for i := range shots {
			shots[i] = shot{
				sess:   w.leader,
				method: http.MethodPost,
				path:   "/api/v1/projects/" + projectID + "/status",
				body:   `{"status":"active"}`,
				tag:    "active",
			}
		}
		st.absorb(r, classify(fire(shots)))
		if status, ok := w.projectStatus(w.leader, projectID); ok && status != "active" {
			st.mismatchf("round %d: final status %q, want active", r, status)
		}
	}
	st.report(th)
}

// raceGroupLeader: two candidates are promoted to leader of the same
// leaderless group at once. The one-leader invariant must hold and the loser
// must get a clean 409 even when only the DB partial-unique index catches it.
func raceGroupLeader(w *raceWorld, th *thresholds, rounds int) {
	st := newStats("group-single-leader")
	for r := 0; r < rounds; r++ {
		resp, err := w.hr2.mustDo(http.MethodPost, "/api/v1/groups", jsonBody(map[string]any{
			"name":        fmt.Sprintf("Race Leaderless %d-%s", r, w.runID),
			"description": "concurrent writer probe",
			"kind":        "standard",
		}), "create leaderless group")
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		groupID := idOf(resp)
		for _, candidate := range w.candidateIDs {
			if _, err = w.hr2.mustDo(http.MethodPost, "/api/v1/groups/"+groupID+"/members",
				jsonBody(map[string]any{"user_id": candidate, "role": "member"}),
				"add candidate"); err != nil {
				break
			}
		}
		if err != nil {
			st.setupFailf(r, err)
			continue
		}
		shots := make([]shot, len(w.candidateIDs))
		for i, candidate := range w.candidateIDs {
			shots[i] = shot{
				sess:   w.hr2,
				method: http.MethodPatch,
				path:   "/api/v1/groups/" + groupID + "/members/" + candidate,
				body:   `{"role":"leader"}`,
				tag:    candidate,
			}
		}
		winner := st.absorb(r, classify(fire(shots)))
		leaders, ok := w.activeLeaderIDs(w.hr2, groupID)
		if !ok {
			st.mismatchf("round %d: cannot read final roster", r)
			continue
		}
		if len(leaders) > 1 {
			st.mismatchf("round %d: %d active leaders, invariant broken", r, len(leaders))
		}
		if winner != nil && (len(leaders) != 1 || leaders[0] != winner.tag) {
			st.mismatchf("round %d: promotion winner %s is not the stored leader %v",
				r, winner.tag, leaders)
		}
	}
	st.report(th)
}

// raceUserDeactivate: N identical deactivations of the same active user.
// Active -> Deactivated fires once; the round ends with a reactivate reset.
func raceUserDeactivate(w *raceWorld, th *thresholds, writers, rounds int) {
	st := newStats("user-deactivate")
	for r := 0; r < rounds; r++ {
		shots := make([]shot, writers)
		for i := range shots {
			shots[i] = shot{
				sess:   w.hr2,
				method: http.MethodPost,
				path:   "/api/v1/users/" + w.victimID + "/deactivate",
				tag:    "deactivate",
			}
		}
		st.absorb(r, classify(fire(shots)))
		if status, ok := w.userStatus(w.hr2, w.victimID); ok && status != "deactivated" {
			st.mismatchf("round %d: final status %q, want deactivated", r, status)
		}
		if _, err := w.hr2.mustDo(http.MethodPost, "/api/v1/users/"+w.victimID+"/reactivate",
			"", "reactivate victim"); err != nil {
			st.setupFailf(r, err)
			break
		}
	}
	st.report(th)
}

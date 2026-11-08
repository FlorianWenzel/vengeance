package store

import (
	"context"
	"encoding/json"
	"net/url"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// Masked is what secrets look like on the way out.
const Masked = "••••"

// Channel is a notification destination.
type Channel struct {
	ID        int64             `json:"id"`
	Name      string            `json:"name"`
	Kind      string            `json:"kind"`
	Config    map[string]string `json:"config"`
	CreatedAt time.Time         `json:"createdAt"`
}

var channelSecrets = map[string][]string{"ntfy": {"token"}, "webhook": {"secret"}}

// Masked returns a copy safe to send to the browser.
func (c Channel) Masked() Channel {
	cfg := map[string]string{}
	for k, v := range c.Config {
		cfg[k] = v
	}
	for _, k := range channelSecrets[c.Kind] {
		if cfg[k] != "" {
			cfg[k] = Masked
		}
	}
	c.Config = cfg
	return c
}

func validateChannel(kind string, cfg map[string]string) error {
	switch kind {
	case "email":
		to := strings.TrimSpace(cfg["to"])
		if to == "" || !strings.Contains(to, "@") {
			return invalid("email channels need a recipient address")
		}
	case "ntfy":
		if cfg["server"] == "" {
			cfg["server"] = "https://ntfy.sh"
		}
		if u, err := url.Parse(cfg["server"]); err != nil || (u.Scheme != "https" && u.Scheme != "http") || u.Host == "" {
			return invalid("ntfy server must be an http(s) URL")
		}
		if strings.TrimSpace(cfg["topic"]) == "" || strings.ContainsAny(cfg["topic"], "/ ") {
			return invalid("ntfy topic is required and cannot contain slashes or spaces")
		}
	case "webhook":
		if u, err := url.Parse(cfg["url"]); err != nil || (u.Scheme != "https" && u.Scheme != "http") || u.Host == "" {
			return invalid("webhook URL must be an http(s) URL")
		}
	default:
		return invalid("kind must be email, ntfy or webhook")
	}
	return nil
}

// ListChannels returns channels with secrets in clear (callers mask).
func (s *Store) ListChannels(ctx context.Context) ([]Channel, error) {
	rows, err := s.DB.Query(ctx, `SELECT id, name, kind, config, created_at FROM channels ORDER BY name, id`)
	if err != nil {
		return nil, err
	}
	return pgx.CollectRows(rows, func(r pgx.CollectableRow) (Channel, error) {
		var c Channel
		var raw []byte
		if err := r.Scan(&c.ID, &c.Name, &c.Kind, &raw, &c.CreatedAt); err != nil {
			return c, err
		}
		c.Config = map[string]string{}
		_ = json.Unmarshal(raw, &c.Config)
		return c, nil
	})
}

// GetChannel returns one channel with secrets.
func (s *Store) GetChannel(ctx context.Context, id int64) (*Channel, error) {
	all, err := s.ListChannels(ctx)
	if err != nil {
		return nil, err
	}
	for _, c := range all {
		if c.ID == id {
			return &c, nil
		}
	}
	return nil, ErrNotFound
}

// ChannelInput creates or edits a channel.
type ChannelInput struct {
	Name   *string           `json:"name"`
	Kind   *string           `json:"kind"`
	Config map[string]string `json:"config"`
}

// SaveChannel creates (id 0) or updates a channel. Masked secrets keep their stored value.
func (s *Store) SaveChannel(ctx context.Context, id int64, in ChannelInput) (*Channel, error) {
	cur := &Channel{Config: map[string]string{}}
	if id != 0 {
		c, err := s.GetChannel(ctx, id)
		if err != nil {
			return nil, err
		}
		cur = c
	}
	if in.Name != nil {
		cur.Name = strings.TrimSpace(*in.Name)
	}
	if in.Kind != nil {
		if id != 0 && *in.Kind != cur.Kind {
			cur.Config = map[string]string{}
		}
		cur.Kind = *in.Kind
	}
	if cur.Name == "" {
		return nil, invalid("name is required")
	}
	if in.Config != nil {
		next := map[string]string{}
		for k, v := range in.Config {
			v = strings.TrimSpace(v)
			if v == Masked {
				v = cur.Config[k]
			}
			next[k] = v
		}
		cur.Config = next
	}
	if err := validateChannel(cur.Kind, cur.Config); err != nil {
		return nil, err
	}
	raw, _ := json.Marshal(cur.Config)
	if id == 0 {
		if err := s.DB.QueryRow(ctx, `INSERT INTO channels (name, kind, config) VALUES ($1,$2,$3) RETURNING id`, cur.Name, cur.Kind, raw).Scan(&id); err != nil {
			return nil, err
		}
	} else if _, err := s.DB.Exec(ctx, `UPDATE channels SET name=$2, kind=$3, config=$4 WHERE id=$1`, id, cur.Name, cur.Kind, raw); err != nil {
		return nil, err
	}
	return s.GetChannel(ctx, id)
}

// DeleteChannel removes a channel and its rules.
func (s *Store) DeleteChannel(ctx context.Context, id int64) error {
	ct, err := s.DB.Exec(ctx, `DELETE FROM channels WHERE id=$1`, id)
	if err == nil && ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return err
}

// RuleFilter narrows which events a rule fires for.
type RuleFilter struct {
	MaxPriority  int      `json:"maxPriority"`
	ProjectID    *int64   `json:"projectId"`
	Tags         []string `json:"tags"`
	FixAvailable *bool    `json:"fixAvailable"`
}

// Rule is a notification rule.
type Rule struct {
	ID          int64      `json:"id"`
	Name        string     `json:"name"`
	Trigger     string     `json:"trigger"`
	Filter      RuleFilter `json:"filter"`
	Mode        string     `json:"mode"`
	ChannelID   int64      `json:"channelId"`
	ChannelName string     `json:"channelName"`
	Enabled     bool       `json:"enabled"`
	CreatedAt   time.Time  `json:"createdAt"`
}

// Triggers are the rule triggers.
var Triggers = []string{"new_finding", "risk_increase", "stale_source", "accepted_expired", "coverage_gap"}

// ListRules lists rules; trigger "" means all.
func (s *Store) ListRules(ctx context.Context, trigger string) ([]Rule, error) {
	q := `SELECT r.id, r.name, r.trigger, r.filter, r.mode, r.channel_id, c.name, r.enabled, r.created_at
		FROM rules r JOIN channels c ON c.id=r.channel_id`
	args := []any{}
	if trigger != "" {
		q += ` WHERE r.trigger=$1 AND r.enabled`
		args = append(args, trigger)
	}
	rows, err := s.DB.Query(ctx, q+` ORDER BY r.name, r.id`, args...)
	if err != nil {
		return nil, err
	}
	return pgx.CollectRows(rows, func(r pgx.CollectableRow) (Rule, error) {
		var x Rule
		var raw []byte
		if err := r.Scan(&x.ID, &x.Name, &x.Trigger, &raw, &x.Mode, &x.ChannelID, &x.ChannelName, &x.Enabled, &x.CreatedAt); err != nil {
			return x, err
		}
		_ = json.Unmarshal(raw, &x.Filter)
		if x.Filter.MaxPriority == 0 {
			x.Filter.MaxPriority = 4
		}
		if x.Filter.Tags == nil {
			x.Filter.Tags = []string{}
		}
		return x, nil
	})
}

// RuleInput creates or edits a rule.
type RuleInput struct {
	Name      *string     `json:"name"`
	Trigger   *string     `json:"trigger"`
	Filter    *RuleFilter `json:"filter"`
	Mode      *string     `json:"mode"`
	ChannelID *int64      `json:"channelId"`
	Enabled   *bool       `json:"enabled"`
}

// SaveRule creates (id 0) or updates a rule.
func (s *Store) SaveRule(ctx context.Context, id int64, in RuleInput) (*Rule, error) {
	cur := Rule{Mode: "instant", Enabled: true, Filter: RuleFilter{MaxPriority: 4, Tags: []string{}}}
	if id != 0 {
		all, err := s.ListRules(ctx, "")
		if err != nil {
			return nil, err
		}
		found := false
		for _, r := range all {
			if r.ID == id {
				cur, found = r, true
			}
		}
		if !found {
			return nil, ErrNotFound
		}
	}
	if in.Name != nil {
		cur.Name = strings.TrimSpace(*in.Name)
	}
	if in.Trigger != nil {
		cur.Trigger = *in.Trigger
	}
	if in.Filter != nil {
		cur.Filter = *in.Filter
		if cur.Filter.MaxPriority == 0 {
			cur.Filter.MaxPriority = 4
		}
		cur.Filter.Tags = cleanTags(cur.Filter.Tags)
	}
	if in.Mode != nil {
		cur.Mode = *in.Mode
	}
	if in.ChannelID != nil {
		cur.ChannelID = *in.ChannelID
	}
	if in.Enabled != nil {
		cur.Enabled = *in.Enabled
	}
	switch {
	case cur.Name == "":
		return nil, invalid("name is required")
	case !contains(Triggers, cur.Trigger):
		return nil, invalid("trigger must be one of " + strings.Join(Triggers, ", "))
	case cur.Mode != "instant" && cur.Mode != "daily" && cur.Mode != "weekly":
		return nil, invalid("mode must be instant, daily or weekly")
	case cur.Filter.MaxPriority < 1 || cur.Filter.MaxPriority > 4:
		return nil, invalid("maxPriority must be 1 to 4")
	case cur.ChannelID == 0:
		return nil, invalid("pick a channel")
	}
	raw, _ := json.Marshal(cur.Filter)
	var err error
	if id == 0 {
		err = s.DB.QueryRow(ctx, `INSERT INTO rules (name, trigger, filter, mode, channel_id, enabled) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id`,
			cur.Name, cur.Trigger, raw, cur.Mode, cur.ChannelID, cur.Enabled).Scan(&id)
	} else {
		_, err = s.DB.Exec(ctx, `UPDATE rules SET name=$2, trigger=$3, filter=$4, mode=$5, channel_id=$6, enabled=$7 WHERE id=$1`,
			id, cur.Name, cur.Trigger, raw, cur.Mode, cur.ChannelID, cur.Enabled)
	}
	if err != nil {
		if strings.Contains(err.Error(), "foreign key") {
			return nil, invalid("that channel does not exist")
		}
		return nil, err
	}
	all, err := s.ListRules(ctx, "")
	if err != nil {
		return nil, err
	}
	for _, r := range all {
		if r.ID == id {
			return &r, nil
		}
	}
	return nil, ErrNotFound
}

// DeleteRule removes a rule.
func (s *Store) DeleteRule(ctx context.Context, id int64) error {
	ct, err := s.DB.Exec(ctx, `DELETE FROM rules WHERE id=$1`, id)
	if err == nil && ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return err
}

// Notification is one queued or sent message part.
type Notification struct {
	ID          int64          `json:"id"`
	RuleID      int64          `json:"-"`
	ChannelID   int64          `json:"-"`
	RuleName    string         `json:"ruleName"`
	ChannelName string         `json:"channelName"`
	Subject     string         `json:"subject"`
	Body        string         `json:"-"`
	Data        map[string]any `json:"-"`
	Status      string         `json:"status"`
	Attempts    int            `json:"-"`
	CreatedAt   time.Time      `json:"createdAt"`
	SentAt      *time.Time     `json:"sentAt"`
	Error       *string        `json:"error"`
}

// Enqueue adds a notification unless the same (rule, key) was queued before.
func (s *Store) Enqueue(ctx context.Context, r Rule, key, subject, body string, data map[string]any) (bool, error) {
	status := "pending"
	if r.Mode != "instant" {
		status = "digest"
	}
	raw, _ := json.Marshal(data)
	ct, err := s.DB.Exec(ctx, `INSERT INTO notifications (rule_id, channel_id, dedupe_key, subject, body, data, status)
		VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (rule_id, dedupe_key) DO NOTHING`, r.ID, r.ChannelID, key, subject, body, raw, status)
	if err != nil {
		return false, err
	}
	return ct.RowsAffected() > 0, nil
}

const notifSelect = `SELECT n.id, COALESCE(n.rule_id,0), COALESCE(n.channel_id,0), COALESCE(r.name,'(deleted rule)'), COALESCE(c.name,'(deleted channel)'),
	n.subject, n.body, n.data, n.status, n.attempts, n.created_at, n.sent_at, n.error
	FROM notifications n LEFT JOIN rules r ON r.id=n.rule_id LEFT JOIN channels c ON c.id=n.channel_id `

func collectNotifs(rows pgx.Rows) ([]Notification, error) {
	return pgx.CollectRows(rows, func(r pgx.CollectableRow) (Notification, error) {
		var n Notification
		var raw []byte
		err := r.Scan(&n.ID, &n.RuleID, &n.ChannelID, &n.RuleName, &n.ChannelName, &n.Subject, &n.Body, &raw, &n.Status, &n.Attempts, &n.CreatedAt, &n.SentAt, &n.Error)
		_ = json.Unmarshal(raw, &n.Data)
		return n, err
	})
}

// ListNotifications returns the log.
func (s *Store) ListNotifications(ctx context.Context, limit int) ([]Notification, error) {
	if limit <= 0 || limit > 500 {
		limit = 50
	}
	rows, err := s.DB.Query(ctx, notifSelect+`ORDER BY n.created_at DESC, n.id DESC LIMIT $1`, limit)
	if err != nil {
		return nil, err
	}
	return collectNotifs(rows)
}

// DueNotifications returns pending instant messages whose retry time has come.
func (s *Store) DueNotifications(ctx context.Context) ([]Notification, error) {
	rows, err := s.DB.Query(ctx, notifSelect+`WHERE n.status='pending' AND n.rule_id IS NOT NULL AND n.channel_id IS NOT NULL
		AND n.created_at + (interval '1 minute' * (power(2, n.attempts) - 1)) <= $1 ORDER BY n.id LIMIT 500`, s.Now())
	if err != nil {
		return nil, err
	}
	return collectNotifs(rows)
}

// DigestNotifications returns queued digest parts for a rule.
func (s *Store) DigestNotifications(ctx context.Context, ruleID int64) ([]Notification, error) {
	rows, err := s.DB.Query(ctx, notifSelect+`WHERE n.status='digest' AND n.rule_id=$1 ORDER BY n.id LIMIT 1000`, ruleID)
	if err != nil {
		return nil, err
	}
	return collectNotifs(rows)
}

// MarkNotifications records a send attempt for a batch.
func (s *Store) MarkNotifications(ctx context.Context, ids []int64, sendErr error) error {
	if sendErr == nil {
		_, err := s.DB.Exec(ctx, `UPDATE notifications SET status='sent', sent_at=$2, error=NULL, attempts=attempts+1 WHERE id = ANY($1)`, ids, s.Now())
		return err
	}
	msg := sendErr.Error()
	if len(msg) > 500 {
		msg = msg[:500]
	}
	_, err := s.DB.Exec(ctx, `UPDATE notifications SET attempts=attempts+1, error=$2,
		status = CASE WHEN attempts+1 >= 5 THEN 'failed' ELSE status END WHERE id = ANY($1)`, ids, msg)
	return err
}

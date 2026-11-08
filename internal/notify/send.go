// Package notify evaluates rules, queues messages and delivers them to channels.
package notify

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"crypto/tls"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"mime"
	"net"
	"net/http"
	"net/smtp"
	"strings"
	"time"

	"github.com/FlorianWenzel/vengeance/internal/store"
)

// SMTP configures outgoing mail.
type SMTP struct {
	Host     string
	Port     string
	User     string
	Password string
	From     string
	TLS      string // starttls (default), tls, none
}

// Message is what a channel delivers.
type Message struct {
	Subject  string           `json:"subject"`
	Body     string           `json:"body"`
	Priority int              `json:"priority"` // 1-4, 0 = n/a
	Link     string           `json:"link"`
	Event    string           `json:"event"`
	Items    []map[string]any `json:"items"`
}

// Sender delivers messages.
type Sender struct {
	SMTP   SMTP
	Client *http.Client
}

// Send delivers one message to one channel.
func (s *Sender) Send(ctx context.Context, ch store.Channel, m Message) error {
	switch ch.Kind {
	case "email":
		return s.email(ch.Config["to"], m)
	case "ntfy":
		return s.ntfy(ctx, ch.Config, m)
	case "webhook":
		return s.webhook(ctx, ch.Config, m)
	}
	return fmt.Errorf("unknown channel kind %q", ch.Kind)
}

func (s *Sender) client() *http.Client {
	if s.Client != nil {
		return s.Client
	}
	return &http.Client{Timeout: 15 * time.Second}
}

func (s *Sender) ntfy(ctx context.Context, cfg map[string]string, m Message) error {
	u := strings.TrimRight(cfg["server"], "/") + "/" + cfg["topic"]
	body := m.Body
	if len(body) > 3500 {
		body = body[:3500] + "\n…"
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, u, strings.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Title", mime.QEncoding.Encode("utf-8", m.Subject))
	prio := "default"
	switch m.Priority {
	case 1:
		prio = "urgent"
	case 2:
		prio = "high"
	}
	req.Header.Set("Priority", prio)
	req.Header.Set("Tags", "rotating_light")
	if m.Link != "" {
		req.Header.Set("Click", m.Link)
	}
	if t := cfg["token"]; t != "" {
		req.Header.Set("Authorization", "Bearer "+t)
	}
	return s.do(req)
}

func (s *Sender) webhook(ctx context.Context, cfg map[string]string, m Message) error {
	payload, _ := json.Marshal(m)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, cfg["url"], bytes.NewReader(payload))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", "Vengeance")
	if sec := cfg["secret"]; sec != "" {
		mac := hmac.New(sha256.New, []byte(sec))
		mac.Write(payload)
		req.Header.Set("X-Vengeance-Signature", "sha256="+hex.EncodeToString(mac.Sum(nil)))
	}
	return s.do(req)
}

func (s *Sender) do(req *http.Request) error {
	resp, err := s.client().Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 300 {
		b, _ := io.ReadAll(io.LimitReader(resp.Body, 300))
		return fmt.Errorf("%s answered %d: %s", req.URL.Host, resp.StatusCode, strings.TrimSpace(string(b)))
	}
	return nil
}

func (s *Sender) email(to string, m Message) error {
	c := s.SMTP
	if c.Host == "" || c.From == "" {
		return fmt.Errorf("email is not configured on the server (VENGEANCE_SMTP_HOST, VENGEANCE_SMTP_FROM)")
	}
	port := c.Port
	if port == "" {
		port = "587"
		if c.TLS == "tls" {
			port = "465"
		}
	}
	addr := net.JoinHostPort(c.Host, port)
	var msg bytes.Buffer
	fmt.Fprintf(&msg, "From: %s\r\nTo: %s\r\nSubject: %s\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\nDate: %s\r\n\r\n",
		c.From, to, mime.QEncoding.Encode("utf-8", m.Subject), time.Now().Format(time.RFC1123Z))
	msg.WriteString(strings.ReplaceAll(m.Body, "\n", "\r\n"))
	if m.Link != "" {
		msg.WriteString("\r\n\r\n" + m.Link + "\r\n")
	}

	var conn net.Conn
	var err error
	d := &net.Dialer{Timeout: 15 * time.Second}
	if c.TLS == "tls" {
		conn, err = tls.DialWithDialer(d, "tcp", addr, &tls.Config{ServerName: c.Host, MinVersion: tls.VersionTLS12})
	} else {
		conn, err = d.Dial("tcp", addr)
	}
	if err != nil {
		return err
	}
	_ = conn.SetDeadline(time.Now().Add(30 * time.Second))
	cl, err := smtp.NewClient(conn, c.Host)
	if err != nil {
		conn.Close()
		return err
	}
	defer cl.Close()
	if c.TLS != "tls" && c.TLS != "none" {
		if err := cl.StartTLS(&tls.Config{ServerName: c.Host, MinVersion: tls.VersionTLS12}); err != nil {
			return fmt.Errorf("starttls: %w", err)
		}
	}
	if c.User != "" {
		if err := cl.Auth(smtp.PlainAuth("", c.User, c.Password, c.Host)); err != nil {
			return fmt.Errorf("smtp auth: %w", err)
		}
	}
	from := c.From
	if i := strings.LastIndex(from, "<"); i >= 0 {
		from = strings.TrimSuffix(from[i+1:], ">")
	}
	if err := cl.Mail(from); err != nil {
		return err
	}
	if err := cl.Rcpt(to); err != nil {
		return err
	}
	w, err := cl.Data()
	if err != nil {
		return err
	}
	if _, err := w.Write(msg.Bytes()); err != nil {
		return err
	}
	if err := w.Close(); err != nil {
		return err
	}
	return cl.Quit()
}

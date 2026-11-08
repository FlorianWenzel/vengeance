package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"strings"
	"time"
)

func push(args []string) error {
	fs := flag.NewFlagSet("push", flag.ContinueOnError)
	srv := fs.String("url", os.Getenv("VENGEANCE_URL"), "server URL (env VENGEANCE_URL)")
	format := fs.String("format", "", "report format: trivy or sarif (default: guessed from the file)")
	asset := fs.String("asset", "", "asset as kind:identifier, e.g. image:ghcr.io/me/app (default: guessed)")
	name := fs.String("name", "", "display name for a new asset")
	project := fs.String("project", "", "project for a new asset")
	scanners := fs.String("scanners", "", "comma list of vuln,secret,misconfig,sast that this scan covered (default: guessed)")
	digest := fs.String("digest", "", "image digest that was scanned")
	fs.Usage = func() {
		fmt.Fprintln(os.Stderr, "Usage: vengeance push [flags] FILE   (FILE may be - for stdin)\n\nThe token comes from VENGEANCE_TOKEN.\n\nFlags:")
		fs.PrintDefaults()
	}
	if err := fs.Parse(args); err != nil {
		return err
	}
	if fs.NArg() != 1 {
		fs.Usage()
		return fmt.Errorf("expected exactly one report file")
	}
	token := os.Getenv("VENGEANCE_TOKEN")
	if *srv == "" || token == "" {
		return fmt.Errorf("set VENGEANCE_URL (or --url) and VENGEANCE_TOKEN")
	}
	var body []byte
	var err error
	if fs.Arg(0) == "-" {
		body, err = io.ReadAll(os.Stdin)
	} else {
		body, err = os.ReadFile(fs.Arg(0))
	}
	if err != nil {
		return err
	}
	if *format == "" {
		*format = guessFormat(body)
	}
	if *asset == "" && *format == "trivy" {
		*asset = guessAsset(body)
	}
	q := url.Values{}
	q.Set("format", *format)
	for k, v := range map[string]string{"asset": *asset, "name": *name, "project": *project, "scanners": *scanners, "digest": *digest} {
		if v != "" {
			q.Set(k, v)
		}
	}
	u := strings.TrimRight(*srv, "/") + "/api/v1/ingest?" + q.Encode()
	var lastErr error
	for attempt := 1; attempt <= 3; attempt++ {
		req, _ := http.NewRequest(http.MethodPost, u, bytes.NewReader(body))
		req.Header.Set("Authorization", "Bearer "+token)
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("User-Agent", "vengeance-cli/"+version)
		resp, err := (&http.Client{Timeout: 5 * time.Minute}).Do(req)
		if err != nil {
			lastErr = err
		} else {
			out, _ := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
			resp.Body.Close()
			if resp.StatusCode == 200 {
				var r struct{ ScanID, AssetID, New, Reopened, Fixed, Open int }
				_ = json.Unmarshal(out, &r)
				fmt.Printf("uploaded: scan %d, asset %d: %d new, %d reopened, %d fixed, %d open\n", r.ScanID, r.AssetID, r.New, r.Reopened, r.Fixed, r.Open)
				return nil
			}
			var e struct{ Error string }
			_ = json.Unmarshal(out, &e)
			lastErr = fmt.Errorf("server answered %d: %s", resp.StatusCode, firstNonEmpty(e.Error, strings.TrimSpace(string(out))))
			if resp.StatusCode < 500 {
				return lastErr
			}
		}
		time.Sleep(time.Duration(attempt*attempt) * 2 * time.Second)
	}
	return lastErr
}

func firstNonEmpty(s ...string) string {
	for _, v := range s {
		if v != "" {
			return v
		}
	}
	return ""
}

func guessFormat(b []byte) string {
	var probe struct {
		Runs    json.RawMessage `json:"runs"`
		Version string          `json:"version"`
	}
	if json.Unmarshal(b, &probe) == nil && probe.Runs != nil {
		return "sarif"
	}
	return "trivy"
}

// guessAsset turns a Trivy filesystem/repository/rootfs report into a repository or host asset.
func guessAsset(b []byte) string {
	var h struct {
		ArtifactName string
		ArtifactType string
	}
	if json.Unmarshal(b, &h) != nil {
		return ""
	}
	switch h.ArtifactType {
	case "repository":
		if r := normaliseRemote(h.ArtifactName); r != "" {
			return "repository:" + r
		}
	case "filesystem", "vm":
		dir := h.ArtifactName
		if dir == "/" || dir == "" {
			if hn, err := os.Hostname(); err == nil {
				return "host:" + hn
			}
			return ""
		}
		if out, err := exec.Command("git", "-C", dir, "remote", "get-url", "origin").Output(); err == nil {
			if r := normaliseRemote(strings.TrimSpace(string(out))); r != "" {
				return "repository:" + r
			}
		}
		if remote := os.Getenv("GITHUB_REPOSITORY"); remote != "" {
			return "repository:github.com/" + remote
		}
	}
	return ""
}

// normaliseRemote maps git@github.com:o/r.git and https://github.com/o/r to github.com/o/r.
func normaliseRemote(r string) string {
	r = strings.TrimSuffix(strings.TrimSpace(r), ".git")
	if after, ok := strings.CutPrefix(r, "git@"); ok {
		return strings.Replace(after, ":", "/", 1)
	}
	if u, err := url.Parse(r); err == nil && u.Host != "" {
		return u.Host + u.Path
	}
	return ""
}

// Package mcpserver exposes Vengeance to AI assistants over the Model Context Protocol
// (streamable HTTP, stateless). Authentication is a personal API token; read-only tokens
// only see the read tools.
package mcpserver

import (
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/FlorianWenzel/vengeance/internal/store"
)

type ctxKey struct{}

// Handler returns the /mcp HTTP handler.
func Handler(st *store.Store, version string, log *slog.Logger) http.Handler {
	read := newServer(st, version, false)
	write := newServer(st, version, true)
	h := mcp.NewStreamableHTTPHandler(func(r *http.Request) *mcp.Server {
		t, _ := r.Context().Value(ctxKey{}).(*store.APIToken)
		if t == nil {
			return nil
		}
		if t.Scope == "write" {
			return write
		}
		return read
	}, &mcp.StreamableHTTPOptions{Stateless: true, JSONResponse: true, Logger: log})

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
		if !ok || strings.TrimSpace(raw) == "" {
			w.Header().Set("WWW-Authenticate", `Bearer realm="vengeance"`)
			http.Error(w, "send a Vengeance API token as Authorization: Bearer vgp_…", http.StatusUnauthorized)
			return
		}
		t, err := st.APITokenByValue(r.Context(), strings.TrimSpace(raw))
		if err != nil {
			if errors.Is(err, store.ErrNotFound) {
				w.Header().Set("WWW-Authenticate", `Bearer realm="vengeance", error="invalid_token"`)
				http.Error(w, "unknown or revoked API token", http.StatusUnauthorized)
				return
			}
			log.Error("mcp auth", "err", err)
			http.Error(w, "server error", http.StatusInternalServerError)
			return
		}
		h.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), ctxKey{}, t)))
	})
}

func tokenOf(ctx context.Context) *store.APIToken {
	t, _ := ctx.Value(ctxKey{}).(*store.APIToken)
	return t
}

// text returns v as pretty JSON text content.
func text(v any) (*mcp.CallToolResult, any, error) {
	b, err := json.MarshalIndent(v, "", " ")
	if err != nil {
		return nil, nil, err
	}
	return &mcp.CallToolResult{Content: []mcp.Content{&mcp.TextContent{Text: string(b)}}}, nil, nil
}

func ro(title string) *mcp.ToolAnnotations {
	f := false
	return &mcp.ToolAnnotations{Title: title, ReadOnlyHint: true, OpenWorldHint: &f}
}

func rw(title string, destructive, idempotent bool) *mcp.ToolAnnotations {
	f := false
	return &mcp.ToolAnnotations{Title: title, DestructiveHint: &destructive, IdempotentHint: idempotent, OpenWorldHint: &f}
}

const instructions = `Vengeance is a self-hosted security inbox. Scanners (Trivy, SARIF tools) push reports; Vengeance
deduplicates them into findings per asset (repository, image, host, cluster, domain), closes findings a
rescan no longer reports, and ranks them P1 (most urgent) to P4 (no fix available). P1 = known exploited
(CISA KEV), critical with a fix on an internet-facing asset, or a high/critical leaked secret.
Start with vengeance_overview. To work on what matters, list_fixes groups findings by the upgrade that
resolves them. Coverage shows assets missing an expected scan. When asked to inventory repositories or
images, compare what exists elsewhere with list_assets and add missing ones with create_asset.`

// --- inputs ---

type findingsIn struct {
	State      string `json:"state,omitempty" jsonschema:"open (default: new+acknowledged), all, new, acknowledged, fixed, accepted, false_positive"`
	Priorities []int  `json:"priorities,omitempty" jsonschema:"only these priorities, 1-4"`
	AssetID    int64  `json:"asset_id,omitempty" jsonschema:"only this asset"`
	ProjectID  int64  `json:"project_id,omitempty" jsonschema:"only this project"`
	Kind       string `json:"kind,omitempty" jsonschema:"vuln, secret, misconfig or sast"`
	Query      string `json:"query,omitempty" jsonschema:"search advisory id, title, package or asset name"`
	Sort       string `json:"sort,omitempty" jsonschema:"priority (default), lastSeen or firstSeen"`
	Limit      int    `json:"limit,omitempty" jsonschema:"page size, default 25, max 200"`
	Offset     int    `json:"offset,omitempty"`
}

type idIn struct {
	ID int64 `json:"id" jsonschema:"the id"`
}

type projectIn struct {
	ProjectID int64 `json:"project_id,omitempty" jsonschema:"only this project"`
}

type assetsIn struct {
	ProjectID      int64  `json:"project_id,omitempty" jsonschema:"only this project"`
	Kind           string `json:"kind,omitempty" jsonschema:"repository, image, host, cluster, domain or registry"`
	IncludeRetired bool   `json:"include_retired,omitempty"`
}

type empty struct{}

type triageIn struct {
	IDs    []int64 `json:"ids" jsonschema:"finding ids"`
	State  string  `json:"state" jsonschema:"acknowledged, accepted (needs until), false_positive (needs reason), or new to reopen"`
	Reason string  `json:"reason,omitempty" jsonschema:"why; required for false_positive"`
	Until  string  `json:"until,omitempty" jsonschema:"YYYY-MM-DD end date of an accepted risk"`
}

type createAssetIn struct {
	Kind       string   `json:"kind" jsonschema:"repository, image, host, cluster, domain or registry"`
	Identifier string   `json:"identifier" jsonschema:"e.g. github.com/owner/repo, ghcr.io/owner/image, node1, example.com"`
	Name       string   `json:"name,omitempty" jsonschema:"display name"`
	Project    string   `json:"project,omitempty" jsonschema:"project name; created if it does not exist"`
	Tags       []string `json:"tags,omitempty" jsonschema:"e.g. prod, internet-facing (raises priority)"`
}

type updateAssetIn struct {
	ID      int64     `json:"id"`
	Name    *string   `json:"name,omitempty"`
	Project *string   `json:"project,omitempty" jsonschema:"project name; empty string unassigns"`
	Tags    *[]string `json:"tags,omitempty" jsonschema:"replaces all tags"`
	Retired *bool     `json:"retired,omitempty" jsonschema:"true when the asset no longer exists"`
}

func newServer(st *store.Store, version string, write bool) *mcp.Server {
	s := mcp.NewServer(&mcp.Implementation{Name: "vengeance", Title: "Vengeance", Version: version}, &mcp.ServerOptions{Instructions: instructions})

	mcp.AddTool(s, &mcp.Tool{Name: "vengeance_overview", Annotations: ro("Security overview"),
		Description: "Open findings by priority, known-exploited count, the top 5 fixes, coverage score and stale sources."},
		func(ctx context.Context, _ *mcp.CallToolRequest, _ empty) (*mcp.CallToolResult, any, error) {
			o, err := st.Overview(ctx)
			if err != nil {
				return nil, nil, err
			}
			o.Trend = nil // 90 daily points are noise for a model
			return text(o)
		})

	mcp.AddTool(s, &mcp.Tool{Name: "list_findings", Annotations: ro("List findings"),
		Description: "Search findings. Defaults to open findings, most urgent first. Returns a page and the total."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in findingsIn) (*mcp.CallToolResult, any, error) {
			if in.Limit <= 0 {
				in.Limit = 25
			}
			if in.Limit > 200 {
				in.Limit = 200
			}
			items, total, err := st.ListFindings(ctx, store.FindingFilter{State: in.State, Priorities: in.Priorities, AssetID: in.AssetID,
				ProjectID: in.ProjectID, Kind: in.Kind, Q: in.Query, Sort: in.Sort, Limit: in.Limit, Offset: in.Offset})
			if err != nil {
				return nil, nil, err
			}
			return text(map[string]any{"total": total, "offset": in.Offset, "items": items})
		})

	mcp.AddTool(s, &mcp.Tool{Name: "get_finding", Annotations: ro("Finding details"),
		Description: "One finding with the advisory description, references and its history."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in idIn) (*mcp.CallToolResult, any, error) {
			f, err := st.GetFinding(ctx, in.ID)
			if err != nil {
				return nil, nil, err
			}
			return text(f)
		})

	mcp.AddTool(s, &mcp.Tool{Name: "list_fixes", Annotations: ro("Upgrades to make"),
		Description: "Open vulnerabilities grouped by the upgrade that resolves them, with a copy-ready command, most urgent first."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in projectIn) (*mcp.CallToolResult, any, error) {
			f, err := st.Fixes(ctx, in.ProjectID)
			if err != nil {
				return nil, nil, err
			}
			return text(f)
		})

	mcp.AddTool(s, &mcp.Tool{Name: "list_assets", Annotations: ro("Inventory"),
		Description: "The inventory: repositories, images, hosts, clusters and domains with open counts and scan coverage."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in assetsIn) (*mcp.CallToolResult, any, error) {
			a, err := st.ListAssets(ctx, store.AssetFilter{ProjectID: in.ProjectID, Kind: in.Kind, IncludeRetired: in.IncludeRetired})
			if err != nil {
				return nil, nil, err
			}
			return text(a)
		})

	mcp.AddTool(s, &mcp.Tool{Name: "get_asset", Annotations: ro("Asset details"),
		Description: "One asset with its coverage and recent scans."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in idIn) (*mcp.CallToolResult, any, error) {
			a, err := st.GetAsset(ctx, in.ID)
			if err != nil {
				return nil, nil, err
			}
			scans, err := st.AssetScans(ctx, in.ID)
			if err != nil {
				return nil, nil, err
			}
			return text(map[string]any{"asset": a, "scans": scans})
		})

	mcp.AddTool(s, &mcp.Tool{Name: "get_coverage", Annotations: ro("Coverage and gaps"),
		Description: "Coverage score per project and every asset that is missing an expected scan or whose last one is too old."},
		func(ctx context.Context, _ *mcp.CallToolRequest, _ empty) (*mcp.CallToolResult, any, error) {
			c, err := st.Coverage(ctx)
			if err != nil {
				return nil, nil, err
			}
			return text(c)
		})

	mcp.AddTool(s, &mcp.Tool{Name: "list_projects", Annotations: ro("Projects"),
		Description: "Projects with their asset and open finding counts."},
		func(ctx context.Context, _ *mcp.CallToolRequest, _ empty) (*mcp.CallToolResult, any, error) {
			p, err := st.ListProjects(ctx)
			if err != nil {
				return nil, nil, err
			}
			return text(p)
		})

	mcp.AddTool(s, &mcp.Tool{Name: "list_sources", Annotations: ro("Scan sources"),
		Description: "Configured scan sources (CI jobs, nightly scans), when each last reported, and whether it went quiet."},
		func(ctx context.Context, _ *mcp.CallToolRequest, _ empty) (*mcp.CallToolResult, any, error) {
			x, err := st.ListSources(ctx)
			if err != nil {
				return nil, nil, err
			}
			return text(x)
		})

	if !write {
		return s
	}

	mcp.AddTool(s, &mcp.Tool{Name: "triage_findings", Annotations: rw("Triage findings", false, true),
		Description: "Change the state of findings: acknowledge, accept a risk until a date, mark a false positive with a reason, or reopen (state new). Fixed is set automatically by rescans."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in triageIn) (*mcp.CallToolResult, any, error) {
			n, err := st.Triage(ctx, "mcp:"+tokenOf(ctx).Name, store.TriageInput{IDs: in.IDs, State: in.State, Reason: in.Reason, Until: in.Until})
			if err != nil {
				return nil, nil, err
			}
			return text(map[string]int{"updated": n})
		})

	mcp.AddTool(s, &mcp.Tool{Name: "create_asset", Annotations: rw("Add to inventory", false, false),
		Description: "Add a repository, image, host, cluster, domain or registry to the inventory so missing scans show up as coverage gaps. Fails if it already exists."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in createAssetIn) (*mcp.CallToolResult, any, error) {
			var pid *int64
			if in.Project != "" {
				id, err := st.ProjectIDByName(ctx, in.Project)
				if err != nil {
					return nil, nil, err
				}
				pid = &id
			}
			name := in.Name
			a, err := st.CreateAsset(ctx, store.AssetInput{Kind: in.Kind, Identifier: in.Identifier, Name: &name, ProjectID: pid, Tags: &in.Tags})
			if errors.Is(err, store.ErrConflict) {
				return nil, nil, errors.New("an asset with this kind and identifier already exists")
			}
			if err != nil {
				return nil, nil, err
			}
			return text(a)
		})

	mcp.AddTool(s, &mcp.Tool{Name: "update_asset", Annotations: rw("Edit asset", false, true),
		Description: "Rename an asset, move it to a project, replace its tags (internet-facing raises priority) or retire it."},
		func(ctx context.Context, _ *mcp.CallToolRequest, in updateAssetIn) (*mcp.CallToolResult, any, error) {
			upd := store.AssetInput{Name: in.Name, Tags: in.Tags, Retired: in.Retired}
			if in.Project != nil {
				var pid int64
				if strings.TrimSpace(*in.Project) != "" {
					id, err := st.ProjectIDByName(ctx, *in.Project)
					if err != nil {
						return nil, nil, err
					}
					pid = id
				}
				upd.ProjectID = &pid
			}
			a, err := st.UpdateAsset(ctx, in.ID, upd)
			if err != nil {
				return nil, nil, err
			}
			return text(a)
		})
	return s
}

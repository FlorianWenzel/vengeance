package store

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
)

// TopologyIn is what a collector reports about a cluster (see docs/topology.md).
type TopologyIn struct {
	Cluster   string       `json:"cluster"`
	Nodes     []TopoNode   `json:"nodes"`
	Workloads []TopoWorkld `json:"workloads"`
}

// TopoNode is one machine.
type TopoNode struct {
	Name    string `json:"name"`
	Host    string `json:"host"`
	CPU     string `json:"cpu"`
	Memory  string `json:"memory"`
	OS      string `json:"os"`
	Kernel  string `json:"kernel"`
	Kubelet string `json:"kubelet"`
}

// TopoWorkld is one workload and its containers.
type TopoWorkld struct {
	Namespace  string          `json:"namespace"`
	Name       string          `json:"name"`
	Kind       string          `json:"kind"`
	Node       string          `json:"node"`
	Ready      int             `json:"ready"`
	Desired    int             `json:"desired"`
	Containers []TopoContainer `json:"containers"`
}

// TopoContainer is one container's image.
type TopoContainer struct {
	Name    string `json:"name"`
	Image   string `json:"image"`
	ImageID string `json:"imageId"`
}

// SaveTopology replaces the snapshot of a cluster.
func (s *Store) SaveTopology(ctx context.Context, sourceID int64, in TopologyIn) error {
	in.Cluster = strings.TrimSpace(in.Cluster)
	if in.Cluster == "" {
		return invalid("cluster is required")
	}
	if len(in.Nodes) > 200 || len(in.Workloads) > 5000 {
		return invalid("topology too large")
	}
	for _, n := range in.Nodes {
		if n.Name == "" {
			return invalid("every node needs a name")
		}
	}
	raw, _ := json.Marshal(in)
	_, err := s.DB.Exec(ctx, `INSERT INTO topology (cluster, source_id, payload, received_at) VALUES ($1,$2,$3,$4)
		ON CONFLICT (cluster) DO UPDATE SET source_id=EXCLUDED.source_id, payload=EXCLUDED.payload, received_at=EXCLUDED.received_at`,
		in.Cluster, sourceID, raw, s.Now())
	return err
}

// Status summarises an element for the blueprint.
type Status struct {
	Level string `json:"level"`
	Open  Counts `json:"open"`
	Stale bool   `json:"stale"`
}

func (st *Status) merge(o Status) {
	st.Open.P1 += o.Open.P1
	st.Open.P2 += o.Open.P2
	st.Open.P3 += o.Open.P3
	st.Open.P4 += o.Open.P4
	st.Open.Total += o.Open.Total
	st.Stale = st.Stale || o.Stale
	if levelRank(o.Level) > levelRank(st.Level) {
		st.Level = o.Level
	}
}

func levelRank(l string) int {
	switch l {
	case "p1":
		return 6
	case "p2":
		return 5
	case "p3":
		return 4
	case "p4":
		return 3
	case "unknown":
		return 2
	case "ok":
		return 1
	}
	return 0
}

func assetStatus(a *Asset) Status {
	s := Status{Open: a.Open, Stale: !a.Covered}
	switch {
	case a.Open.P1 > 0:
		s.Level = "p1"
	case a.Open.P2 > 0:
		s.Level = "p2"
	case a.Open.P3 > 0:
		s.Level = "p3"
	case a.Open.P4 > 0:
		s.Level = "p4"
	case a.LastScanAt == nil:
		s.Level = "unknown"
	default:
		s.Level = "ok"
	}
	return s
}

var unknownStatus = Status{Level: "unknown", Stale: true}

// Scene types (docs/topology.md §2).
type (
	Scene struct {
		GeneratedAt time.Time  `json:"generatedAt"`
		Hosts       []Host     `json:"hosts"`
		External    []External `json:"external"`
	}
	Host struct {
		ID       string  `json:"id"`
		AssetID  *int64  `json:"assetId"`
		Name     string  `json:"name"`
		Subtitle string  `json:"subtitle"`
		Status   Status  `json:"status"`
		Cluster  *string `json:"cluster"`
		Groups   []Group `json:"groups"`
	}
	Group struct {
		ID        string     `json:"id"`
		Name      string     `json:"name"`
		Status    Status     `json:"status"`
		Workloads []Workload `json:"workloads"`
	}
	Workload struct {
		ID      string  `json:"id"`
		Name    string  `json:"name"`
		Kind    string  `json:"kind"`
		Ready   int     `json:"ready"`
		Desired int     `json:"desired"`
		Status  Status  `json:"status"`
		Images  []Image `json:"images"`
	}
	Image struct {
		ID      string `json:"id"`
		AssetID *int64 `json:"assetId"`
		Name    string `json:"name"`
		Ref     string `json:"ref"`
		Status  Status `json:"status"`
	}
	External struct {
		ID      string `json:"id"`
		AssetID int64  `json:"assetId"`
		Kind    string `json:"kind"`
		Name    string `json:"name"`
		Status  Status `json:"status"`
	}
)

// NormaliseImage maps a running image reference onto the identifier the scanners use.
func NormaliseImage(ref string) string {
	r := ImageRepo(ref)
	r = strings.TrimPrefix(r, "docker.io/library/")
	r = strings.TrimPrefix(r, "docker.io/")
	r = strings.TrimPrefix(r, "index.docker.io/library/")
	return r
}

func memHuman(m string) string {
	var ki int64
	if _, err := fmt.Sscanf(m, "%dKi", &ki); err == nil && ki > 0 {
		return fmt.Sprintf("%.1f GiB", float64(ki)/1024/1024)
	}
	return m
}

// Scene builds the blueprint from the latest topology snapshots and current findings.
func (s *Store) Scene(ctx context.Context) (*Scene, error) {
	assets, err := s.ListAssets(ctx, AssetFilter{})
	if err != nil {
		return nil, err
	}
	byRef := map[string]*Asset{}
	for i := range assets {
		a := &assets[i]
		byRef[a.Kind+":"+a.Identifier] = a
	}
	rows, err := s.DB.Query(ctx, `SELECT payload FROM topology ORDER BY cluster`)
	if err != nil {
		return nil, err
	}
	snaps, err := pgx.CollectRows(rows, func(r pgx.CollectableRow) (TopologyIn, error) {
		var raw []byte
		var t TopologyIn
		if err := r.Scan(&raw); err != nil {
			return t, err
		}
		return t, json.Unmarshal(raw, &t)
	})
	if err != nil {
		return nil, err
	}

	scene := &Scene{GeneratedAt: s.Now().UTC(), Hosts: []Host{}, External: []External{}}
	placedHosts := map[int64]bool{}
	for _, t := range snaps {
		cluster := t.Cluster
		hostIdx := map[string]int{}
		for _, n := range t.Nodes {
			h := Host{ID: "node:" + n.Name, Name: n.Name, Status: unknownStatus, Cluster: &cluster, Groups: []Group{}}
			if a := byRef[n.Host]; a != nil {
				id := a.ID
				h.ID, h.AssetID, h.Name, h.Status = fmt.Sprintf("host:%d", a.ID), &id, a.Name, assetStatus(a)
				placedHosts[a.ID] = true
			}
			parts := []string{}
			for _, p := range []string{n.OS, cpuLabel(n.CPU), memHuman(n.Memory), kubeletLabel(n.Kubelet)} {
				if p != "" {
					parts = append(parts, p)
				}
			}
			h.Subtitle = strings.Join(parts, " · ")
			hostIdx[n.Name] = len(scene.Hosts)
			scene.Hosts = append(scene.Hosts, h)
		}
		groups := map[string]map[string]*Group{} // node -> namespace -> group
		for _, w := range t.Workloads {
			hi, ok := hostIdx[w.Node]
			if !ok {
				continue
			}
			if groups[w.Node] == nil {
				groups[w.Node] = map[string]*Group{}
			}
			g := groups[w.Node][w.Namespace]
			if g == nil {
				g = &Group{ID: "ns:" + cluster + "/" + w.Namespace, Name: w.Namespace, Status: Status{Level: "ok"}, Workloads: []Workload{}}
				groups[w.Node][w.Namespace] = g
			}
			wl := Workload{ID: "wl:" + cluster + "/" + w.Namespace + "/" + w.Kind + "/" + w.Name, Name: w.Name, Kind: w.Kind,
				Ready: w.Ready, Desired: w.Desired, Status: Status{Level: "ok"}, Images: []Image{}}
			seen := map[string]bool{}
			for _, c := range w.Containers {
				norm := NormaliseImage(c.Image)
				if norm == "" || seen[norm] {
					continue
				}
				seen[norm] = true
				img := Image{ID: "img:" + norm, Name: shortName(norm), Ref: c.Image, Status: unknownStatus}
				if a := byRef["image:"+norm]; a != nil {
					id := a.ID
					img.ID, img.AssetID, img.Status = fmt.Sprintf("img:%d", a.ID), &id, assetStatus(a)
				}
				wl.Status.merge(img.Status)
				wl.Images = append(wl.Images, img)
			}
			g.Status.merge(wl.Status)
			g.Workloads = append(g.Workloads, wl)
			_ = hi
		}
		for node, gs := range groups {
			h := &scene.Hosts[hostIdx[node]]
			for _, g := range gs {
				sort.Slice(g.Workloads, func(i, j int) bool { return g.Workloads[i].Name < g.Workloads[j].Name })
				h.Groups = append(h.Groups, *g)
			}
			sort.Slice(h.Groups, func(i, j int) bool { return h.Groups[i].Name < h.Groups[j].Name })
		}
	}
	// hosts known to the inventory but not reporting topology still get a rack
	for i := range assets {
		a := &assets[i]
		if a.Kind == "host" && !placedHosts[a.ID] {
			id := a.ID
			scene.Hosts = append(scene.Hosts, Host{ID: fmt.Sprintf("host:%d", a.ID), AssetID: &id, Name: a.Name,
				Subtitle: "no topology reported", Status: assetStatus(a), Groups: []Group{}})
		}
		switch a.Kind {
		case "repository", "domain", "registry":
			scene.External = append(scene.External, External{ID: fmt.Sprintf("asset:%d", a.ID), AssetID: a.ID, Kind: a.Kind, Name: a.Name, Status: assetStatus(a)})
		}
	}
	return scene, nil
}

func shortName(ref string) string {
	if i := strings.LastIndex(ref, "/"); i >= 0 {
		return ref[i+1:]
	}
	return ref
}

func cpuLabel(c string) string {
	if c == "" {
		return ""
	}
	return c + " CPU"
}

func kubeletLabel(k string) string {
	if k == "" {
		return ""
	}
	if i := strings.Index(k, "+k3s"); i > 0 {
		return "k3s " + k[:i]
	}
	return "kubelet " + k
}

#!/usr/bin/env python3
"""Print this cluster's topology as Vengeance expects it (docs/topology.md §1).

Usage: kubectl get nodes,pods,replicasets,deployments,statefulsets,daemonsets -A -o json \
         | vengeance-topology.py CLUSTER HOST_ASSET [ROUTES_JSON]

ROUTES_JSON is optional output of `kubectl get httproutes,ingresses -A -o json`; it tells which
namespace serves which hostname (the neon doors in the blueprint's walk mode).
"""
import json
import sys

cluster, host_asset = sys.argv[1], sys.argv[2]
items = json.load(sys.stdin)["items"]
by_kind = {}
for it in items:
    by_kind.setdefault(it["kind"], []).append(it)

nodes = []
for n in by_kind.get("Node", []):
    info = n["status"].get("nodeInfo", {})
    cap = n["status"].get("capacity", {})
    nodes.append({"name": n["metadata"]["name"], "host": host_asset if len(by_kind.get("Node", [])) == 1 else f"host:{n['metadata']['name']}",
                  "cpu": cap.get("cpu", ""), "memory": cap.get("memory", ""), "os": info.get("osImage", ""),
                  "kernel": info.get("kernelVersion", ""), "kubelet": info.get("kubeletVersion", "")})

rs_owner = {}
for rs in by_kind.get("ReplicaSet", []):
    for o in rs["metadata"].get("ownerReferences", []):
        rs_owner[(rs["metadata"]["namespace"], rs["metadata"]["name"])] = (o["kind"], o["name"])

desired = {}
for kind in ("Deployment", "StatefulSet"):
    for d in by_kind.get(kind, []):
        desired[(d["metadata"]["namespace"], kind, d["metadata"]["name"])] = d["spec"].get("replicas", 1)
for d in by_kind.get("DaemonSet", []):
    desired[(d["metadata"]["namespace"], "DaemonSet", d["metadata"]["name"])] = d["status"].get("desiredNumberScheduled", 0)

workloads = {}
for p in by_kind.get("Pod", []):
    phase = p["status"].get("phase")
    if phase in ("Succeeded", "Failed"):
        continue
    ns, name = p["metadata"]["namespace"], p["metadata"]["name"]
    kind, wname = "Pod", name
    for o in p["metadata"].get("ownerReferences", []):
        kind, wname = o["kind"], o["name"]
        if kind == "ReplicaSet" and (ns, wname) in rs_owner:
            kind, wname = rs_owner[(ns, wname)]
    if kind == "Job":
        continue
    key = (ns, kind, wname)
    w = workloads.setdefault(key, {"namespace": ns, "name": wname, "kind": kind, "node": p["spec"].get("nodeName", ""),
                                   "ready": 0, "desired": desired.get(key, 0), "containers": {}})
    statuses = {c["name"]: c for c in p["status"].get("containerStatuses", [])}
    if statuses and all(c.get("ready") for c in statuses.values()):
        w["ready"] += 1
    containers = p["spec"]["containers"]
    for c in containers:
        if c["name"] == "istio-proxy" and len(containers) > 1:
            continue  # mesh sidecars would repeat on every workload; gateways keep theirs
        st = statuses.get(c["name"], {})
        w["containers"][c["name"]] = {"name": c["name"], "image": c["image"], "imageId": st.get("imageID", "")}

out = []
for w in workloads.values():
    if not w["desired"]:
        w["desired"] = max(w["ready"], 1)
    w["containers"] = list(w["containers"].values())
    out.append(w)
routes = {}
if len(sys.argv) > 3:
    try:
        with open(sys.argv[3]) as f:
            for r in json.load(f).get("items", []):
                ns = r["metadata"]["namespace"]
                hosts = list(r.get("spec", {}).get("hostnames", []))
                hosts += [rule["host"] for rule in r.get("spec", {}).get("rules", []) if rule.get("host")]
                if hosts:
                    routes.setdefault(ns, set()).update(hosts)
    except (OSError, ValueError, KeyError):
        pass
json.dump({"cluster": cluster, "nodes": nodes, "workloads": out,
           "routes": [{"namespace": ns, "hostnames": sorted(h)} for ns, h in sorted(routes.items())]}, sys.stdout)

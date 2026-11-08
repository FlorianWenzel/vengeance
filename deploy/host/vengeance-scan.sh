#!/usr/bin/env bash
# Nightly scan of a k3s node: the host itself, every image running in the cluster, and the
# cluster's configuration. Results go to Vengeance. Install with deploy/host/README.md.
set -uo pipefail

: "${VENGEANCE_URL:?}" "${VENGEANCE_TOKEN:?}"
HOST_ASSET="${HOST_ASSET:-host:$(hostname)}"
CLUSTER_ASSET="${CLUSTER_ASSET:-cluster:$(hostname)}"
PROJECT="${PROJECT:-}"
export KUBECONFIG="${KUBECONFIG:-/etc/rancher/k3s/k3s.yaml}"
export CONTAINERD_ADDRESS="${CONTAINERD_ADDRESS:-/run/k3s/containerd/containerd.sock}"
export CONTAINERD_NAMESPACE=k8s.io
export TRIVY_CACHE_DIR="${TRIVY_CACHE_DIR:-/var/cache/trivy}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
failed=0
proj=(); [ -n "$PROJECT" ] && proj=(--project "$PROJECT")

log() { echo "[vengeance-scan] $*"; }

trivy image --download-db-only --quiet || { log "trivy DB update failed"; exit 1; }

# 1. The host: OS packages and binaries outside container storage.
log "host $HOST_ASSET"
if trivy rootfs --quiet --skip-db-update --scanners vuln --timeout 30m \
     --skip-dirs /proc,/sys,/run,/var/lib/rancher,/var/lib/kubelet,/var/lib/containerd,/var/cache/trivy,/var/log,/tmp \
     -f json -o "$work/host.json" /; then
  vengeance push --asset "$HOST_ASSET" --scanners vuln "${proj[@]}" "$work/host.json" || failed=1
else
  log "host scan failed"; failed=1
fi

# 2. Every image running in the cluster, by digest, straight from containerd.
kubectl get pods -A -o jsonpath='{range .items[*].status.containerStatuses[*]}{.image}{"|"}{.imageID}{"\n"}{end}' \
  | sort -u > "$work/images.txt"
while IFS='|' read -r image imageid; do
  [ -n "$image" ] || continue
  repo="${image%@*}"; last="${repo##*/}"; [[ "$last" == *:* ]] && repo="${repo%:*}"
  digest="${imageid##*@}"
  case "$repo" in docker.io/library/*) repo="${repo#docker.io/library/}";; docker.io/*) repo="${repo#docker.io/}";; esac
  out="$work/img-$(echo "$repo" | tr '/:' '__').json"
  log "image $repo ($digest)"
  # Read the image from containerd; k3s may have discarded compressed layers of some images,
  # so fall back to pulling it from its registry by digest.
  if trivy image --quiet --skip-db-update --image-src containerd --scanners vuln,secret --timeout 15m \
       -f json -o "$out" "$image" 2>"$work/err" ||
     { log "containerd read failed, pulling $repo@$digest from the registry"; \
       trivy image --quiet --skip-db-update --image-src remote --scanners vuln,secret --timeout 15m \
       -f json -o "$out" "$repo@$digest" 2>>"$work/err"; }; then
    vengeance push --asset "image:$repo" --digest "$digest" --scanners vuln,secret "${proj[@]}" "$out" || failed=1
  else
    log "scan of $image failed: $(tail -c 300 "$work/err")"; failed=1
  fi
done < "$work/images.txt"

# 3. Cluster configuration (workload security settings, RBAC).
log "cluster $CLUSTER_ASSET"
if trivy k8s --quiet --skip-db-update --scanners misconfig --disable-node-collector --report all --timeout 30m \
     -f json -o "$work/k8s.json"; then
  vengeance push --asset "$CLUSTER_ASSET" --scanners misconfig "${proj[@]}" "$work/k8s.json" || failed=1
else
  log "cluster scan failed"; failed=1
fi

# 4. Topology for the infrastructure blueprint: nodes, namespaces, workloads, images.
log "topology $CLUSTER_ASSET"
if kubectl get nodes,pods,replicasets,deployments,statefulsets,daemonsets -A -o json \
     | vengeance-topology "${CLUSTER_ASSET#cluster:}" "$HOST_ASSET" > "$work/topology.json"; then
  curl -fsS -X POST -H "Authorization: Bearer $VENGEANCE_TOKEN" -H "Content-Type: application/json" \
    --data-binary @"$work/topology.json" "$VENGEANCE_URL/api/v1/topology" >/dev/null || { log "topology upload failed"; failed=1; }
else
  log "topology collection failed"; failed=1
fi

exit $failed

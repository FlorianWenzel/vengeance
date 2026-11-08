# Scanning a k3s node every night

`vengeance-scan.sh` scans the host (`trivy rootfs`), every image running in the cluster (straight
from containerd, by digest) and the cluster configuration (`trivy k8s --scanners misconfig`), and
pushes each result to Vengeance as its own asset. A systemd timer runs it at 03:30.

```bash
# trivy and the vengeance CLI on the node
install -m 0755 vengeance /usr/local/bin/vengeance          # from a GitHub release
install -m 0755 vengeance-scan.sh /usr/local/bin/vengeance-scan
install -m 0755 vengeance-topology.py /usr/local/bin/vengeance-topology   # feeds the 3D blueprint
install -d -m 0700 /etc/vengeance
cat > /etc/vengeance/scan.env <<ENV
VENGEANCE_URL=https://vengeance.example.com
VENGEANCE_TOKEN=vg_…            # a source created under Sources
HOST_ASSET=host:node1
CLUSTER_ASSET=cluster:prod
PROJECT=infrastructure
ENV
chmod 600 /etc/vengeance/scan.env
cp vengeance-scan.service vengeance-scan.timer /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now vengeance-scan.timer
systemctl start vengeance-scan.service && journalctl -u vengeance-scan -f   # first run
```

Give the source a stale threshold of about 30 hours: if the timer stops, a "source went quiet"
rule tells you the next morning.

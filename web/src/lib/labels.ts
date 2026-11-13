import type {
  AssetKind,
  ChannelKind,
  Kind,
  Priority,
  RuleMode,
  ScanType,
  Severity,
  State,
  Trigger,
} from "./types";
import type { IconName } from "./icons";

export const PRIORITIES: Priority[] = [1, 2, 3, 4];

export const priorityHint: Record<Priority, string> = {
  1: "Exploited in the wild or critical and exposed. Act now.",
  2: "Critical or high with a fix available.",
  3: "Lower severity with a fix available.",
  4: "No fix available yet.",
};

export const stateLabel: Record<State, string> = {
  new: "New",
  acknowledged: "Acknowledged",
  accepted: "Risk accepted",
  false_positive: "False positive",
  fixed: "Fixed",
};

export const stateFilterOptions: { value: string; label: string }[] = [
  { value: "open", label: "Open (new + acknowledged)" },
  { value: "new", label: "New" },
  { value: "acknowledged", label: "Acknowledged" },
  { value: "accepted", label: "Risk accepted" },
  { value: "false_positive", label: "False positive" },
  { value: "fixed", label: "Fixed" },
  { value: "all", label: "All states" },
];

export const kindLabel: Record<Kind, string> = {
  vuln: "Vulnerability",
  secret: "Secret",
  misconfig: "Misconfiguration",
  sast: "Code (SAST)",
};

export const KINDS: Kind[] = ["vuln", "secret", "misconfig", "sast"];
export const SCAN_TYPES: ScanType[] = ["vuln", "secret", "misconfig", "sast"];

export const scanTypeLabel: Record<ScanType, string> = {
  vuln: "Vulns",
  secret: "Secrets",
  misconfig: "Misconfig",
  sast: "SAST",
};

export const ASSET_KINDS: AssetKind[] = [
  "repository",
  "image",
  "host",
  "cluster",
  "domain",
  "registry",
];

export const assetKindLabel: Record<AssetKind, string> = {
  repository: "Repository",
  image: "Container image",
  host: "Host",
  cluster: "Cluster",
  domain: "Domain",
  registry: "Registry",
};

export const assetKindIcon: Record<AssetKind, IconName> = {
  repository: "git-branch",
  image: "container",
  host: "server",
  cluster: "network",
  domain: "globe",
  registry: "database",
};

export const assetIdentifierHint: Record<AssetKind, string> = {
  repository: "github.com/acme/api",
  image: "ghcr.io/acme/api",
  host: "node1.internal",
  cluster: "prod",
  domain: "acme.example",
  registry: "ghcr.io/acme",
};

export const severityOrder: Severity[] = [
  "critical",
  "high",
  "medium",
  "low",
  "unknown",
];

export const triggerLabel: Record<Trigger, string> = {
  new_finding: "New finding",
  risk_increase: "Finding became riskier",
  stale_source: "Source went quiet",
  accepted_expired: "Accepted risk expired",
  coverage_gap: "Coverage gap",
};

export const TRIGGERS = Object.keys(triggerLabel) as Trigger[];

export const modeLabel: Record<RuleMode, string> = {
  instant: "Instant",
  daily: "Daily digest",
  weekly: "Weekly digest",
};

export const maxPriorityLabel: Record<Priority, string> = {
  1: "P1 only",
  2: "P1–P2",
  3: "P1–P3",
  4: "All priorities",
};

export const channelKindLabel: Record<ChannelKind, string> = {
  email: "Email",
  ntfy: "ntfy",
  webhook: "Webhook",
};

export const SECRET_MASK = "••••";

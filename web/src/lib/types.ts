// Mirrors docs/api.md (Vengeance HTTP API v1). Keep in sync with the contract.

export type Priority = 1 | 2 | 3 | 4
export type State = 'new' | 'acknowledged' | 'fixed' | 'accepted' | 'false_positive'
export type StateFilter = 'open' | 'all' | State
export type Kind = 'vuln' | 'secret' | 'misconfig' | 'sast'
export type AssetKind = 'repository' | 'image' | 'host' | 'cluster' | 'domain' | 'registry'
export type ScanType = 'vuln' | 'secret' | 'misconfig' | 'sast'
export type Severity = 'critical' | 'high' | 'medium' | 'low' | 'unknown'

export interface Me {
  id: number | string
  email: string
  name: string
}

export interface OpenCounts {
  p1: number
  p2: number
  p3: number
  p4: number
  total: number
}

export interface Finding {
  id: number
  assetId: number
  assetName: string
  assetKind: AssetKind
  projectName: string | null
  kind: Kind
  advisoryId: string
  title: string
  severity: Severity
  priority: Priority
  pkgName: string | null
  pkgEcosystem: string | null
  installedVersion: string | null
  fixedVersion: string | null
  target: string
  state: State
  stateReason: string | null
  acceptedUntil: string | null
  firstSeen: string
  lastSeen: string
  fixedAt: string | null
  kev: boolean
  cvss: number | null
  sources: string[]
}

export interface FindingEvent {
  at: string
  actor: string
  fromState: State | null
  toState: State
  note: string | null
}

export interface FindingDetail extends Finding {
  description: string
  references: string[]
  publishedAt: string | null
  events: FindingEvent[]
}

export interface FindingList {
  items: Finding[]
  total: number
}

export interface TriageRequest {
  ids: number[]
  state: State
  reason?: string
  until?: string
}

export interface FixGroup {
  key: string
  assetId: number
  assetName: string
  pkgName: string
  ecosystem: string
  installedVersion: string
  targetVersion: string
  command: string
  findings: number
  maxPriority: Priority
  kev: boolean
  advisories: string[]
}

export interface Overview {
  open: OpenCounts
  kev: number
  trend: { date: string; open: number }[]
  weekly: { week: string; new: number; fixed: number }[]
  queue: FixGroup[]
  coverage: { score: number; gaps: number; assets: number }
  staleSources: number
}

export interface CoverageCell {
  scanType: ScanType
  lastAt: string | null
  maxAgeDays: number
  ok: boolean
}

export interface Asset {
  id: number
  kind: AssetKind
  identifier: string
  name: string
  projectId: number | null
  projectName: string | null
  tags: string[]
  retiredAt: string | null
  createdAt: string
  lastScanAt: string | null
  discovered: boolean
  open: OpenCounts
  coverage: CoverageCell[]
  covered: boolean
}

export interface Scan {
  id: number
  sourceName: string
  format: string
  scanTypes: ScanType[]
  digest: string | null
  receivedAt: string
  findingCount: number
}

export interface AssetDetail extends Asset {
  scans: Scan[]
}

export interface AssetCreate {
  kind: AssetKind
  identifier: string
  name?: string
  projectId?: number | null
  tags?: string[]
}

export interface AssetPatch {
  name?: string
  projectId?: number | null
  tags?: string[]
  retired?: boolean
}

export interface Project {
  id: number
  name: string
  assets: number
  open: number
}

export interface Coverage {
  score: number
  byProject: { projectId: number | null; name: string; score: number; assets: number; gaps: number }[]
  gaps: {
    assetId: number
    assetName: string
    assetKind: AssetKind
    scanType: ScanType
    lastAt: string | null
    maxAgeDays: number
  }[]
}

export interface Expectation {
  kind: AssetKind
  scanType: ScanType
  maxAgeDays: number
}

export interface Source {
  id: number
  name: string
  tokenPrefix: string
  lastSeenAt: string | null
  staleAfterHours: number
  stale: boolean
  scans: number
  createdAt: string
}

export interface ApiToken {
  id: number
  name: string
  tokenPrefix: string
  scope: 'read' | 'write'
  createdBy: string | null
  createdAt: string
  lastUsedAt: string | null
}

export type ChannelKind = 'email' | 'ntfy' | 'webhook'

export interface ChannelConfig {
  to?: string
  server?: string
  topic?: string
  token?: string
  url?: string
  secret?: string
}

export interface Channel {
  id: number
  name: string
  kind: ChannelKind
  config: ChannelConfig
  createdAt: string
}

export type Trigger = 'new_finding' | 'risk_increase' | 'stale_source' | 'accepted_expired' | 'coverage_gap'
export type RuleMode = 'instant' | 'daily' | 'weekly'

export interface RuleFilter {
  maxPriority: Priority
  projectId: number | null
  tags: string[]
  fixAvailable: boolean | null
}

export interface Rule {
  id: number
  name: string
  trigger: Trigger
  filter: RuleFilter
  mode: RuleMode
  channelId: number
  channelName: string
  enabled: boolean
  createdAt: string
}

export type RuleInput = Pick<Rule, 'name' | 'trigger' | 'filter' | 'mode' | 'channelId' | 'enabled'>

export interface NotificationLog {
  id: number
  ruleName: string
  channelName: string
  subject: string
  status: 'pending' | 'digest' | 'sent' | 'failed'
  createdAt: string
  sentAt: string | null
  error: string | null
}

// --- infrastructure blueprint (docs/topology.md) ---
export type StatusLevel = 'p1' | 'p2' | 'p3' | 'p4' | 'ok' | 'unknown'
export interface TopoStatus {
  level: StatusLevel
  open: OpenCounts
  stale: boolean
}
export interface TopoImage {
  id: string
  assetId: number | null
  name: string
  ref: string
  status: TopoStatus
}
export interface TopoWorkload {
  id: string
  name: string
  kind: string
  ready: number
  desired: number
  status: TopoStatus
  images: TopoImage[]
}
export interface TopoGroup {
  id: string
  name: string
  status: TopoStatus
  workloads: TopoWorkload[]
}
export interface TopoHost {
  id: string
  assetId: number | null
  name: string
  subtitle: string
  status: TopoStatus
  cluster: string | null
  groups: TopoGroup[]
}
export interface TopoExternal {
  id: string
  assetId: number
  kind: AssetKind
  name: string
  status: TopoStatus
}
export interface Scene {
  generatedAt: string
  hosts: TopoHost[]
  external: TopoExternal[]
}

export interface NodeJson {
  node_id: string;
  hostname: string;
  agent_address: string;
  labels: Record<string, string>;
  annotations: Record<string, string>;
  state: string;
  registered_at: string;
  last_heartbeat: string;
  pacgate_version: string;
  uptime_seconds: number;
  policy_hash: string;
  last_heartbeat_age_seconds: number;
}

export interface PolicyJson {
  node_id: string;
  rules_yaml: string;
  policy_hash: string;
  deployed_at: string;
  counters_enabled: boolean;
  rate_limit_enabled: boolean;
  conntrack_enabled: boolean;
}

export interface RuleCounterJson {
  rule_name: string;
  match_count: number;
  byte_count: number;
}

export interface CounterJson {
  node_id: string;
  counters: RuleCounterJson[];
  collected_at: string;
}

export interface PolicyVersionJson {
  version: number;
  node_id: string;
  rules_yaml: string;
  policy_hash: string;
  deployed_at: string;
}

export interface DeploymentJson {
  id: string;
  node_id: string;
  policy_version: number;
  policy_hash: string;
  deployed_at: string;
  result: string;
  message: string;
}

export interface FleetStatusJson {
  total_nodes: number;
  nodes_by_state: Record<string, number>;
  nodes: FleetNodeJson[];
}

export interface FleetNodeJson {
  node_id: string;
  hostname: string;
  state: string;
  policy_hash: string;
  uptime_seconds: number;
  last_heartbeat_age_seconds: number;
  last_deploy_time: string | null;
}

export interface NodeCounterSetJson {
  node_id: string;
  counters: RuleCounterJson[];
  collected_at: string;
}

export interface BatchDeployResultJson {
  total_nodes: number;
  succeeded: number;
  failed: number;
  results: NodeDeployResultJson[];
}

export interface NodeDeployResultJson {
  node_id: string;
  hostname: string;
  success: boolean;
  message: string;
}

export interface FsmDefSummaryJson {
  name: string;
  kind: string;
  description: string;
  state_count: number;
  initial_state: string;
}

export interface FsmDefJson {
  name: string;
  kind: string;
  description: string;
  definition_yaml: string;
}

export interface FsmInstanceJson {
  instance_id: string;
  definition_name: string;
  current_state: string;
  status: string;
  created_at: string;
  updated_at: string;
  deployed_nodes: number;
  failed_nodes: number;
  target_nodes: number;
  history: FsmTransitionJson[];
}

export interface FsmTransitionJson {
  from_state: string;
  to_state: string;
  trigger: string;
  timestamp: string;
  message: string;
}

export interface SuccessResponse {
  success: boolean;
  message: string;
}

export interface DeployResponse {
  success: boolean;
  message: string;
  warnings: string[];
}

export interface RollbackResponse {
  success: boolean;
  message: string;
  version: number;
}

export interface CreateFsmDefResponse {
  success: boolean;
  name: string;
  message: string;
}

export interface StartFsmResponse {
  success: boolean;
  instance_id: string;
  message: string;
}

export interface AdvanceFsmResponse {
  success: boolean;
  state: string;
  message: string;
}

// Persistent event log
export interface PersistentEventJson {
  id: string;
  event_type: string;
  source: string;
  payload: string;
  timestamp: string;
}

// Health
export interface HealthResponse {
  status: string;
  auth_required: boolean;
  role: string;
}

// Phase 9: Node annotations (added to NodeJson above)
// annotations: Record<string, string>

// Phase 9: Audit log
export interface AuditEntryJson {
  id: string;
  timestamp: string;
  actor: string;
  action: string;
  resource_type: string;
  resource_id: string;
  details: string;
}

// Phase 9: Policy templates
export interface PolicyTemplateJson {
  name: string;
  description: string;
  rules_yaml: string;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface PolicyTemplateSummaryJson {
  name: string;
  description: string;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface CreateTemplateResponse {
  success: boolean;
  name: string;
  message: string;
}

// Phase 9: Webhook delivery history
export interface WebhookDeliveryJson {
  id: string;
  instance_id: string;
  url: string;
  method: string;
  status_code: number | null;
  success: boolean;
  duration_ms: number;
  error: string | null;
  attempt: number;
  timestamp: string;
}

// Phase 9: Dry-run deploy
export interface DryRunResultJson {
  valid: boolean;
  validation_errors: string[];
  target_nodes: DryRunNodeJson[];
}

export interface DryRunNodeJson {
  node_id: string;
  hostname: string;
  current_policy_hash: string;
  new_policy_hash: string;
  policy_changed: boolean;
}

export interface DryRunDeployResponse extends DeployResponse {
  dry_run_result?: DryRunResultJson;
}

// SSE event types
export interface NodeEventJson {
  event_type: string;
  node_id: string;
  hostname: string;
  labels: Record<string, string>;
  old_state: string;
  new_state: string;
  timestamp: string;
}

export interface CounterEventJson {
  node_id: string;
  counters: CounterRateJson[];
  collected_at: string;
}

export interface CounterRateJson {
  rule_name: string;
  match_count: number;
  byte_count: number;
  matches_per_second: number;
  bytes_per_second: number;
}

export interface FsmEventJson {
  event_type: string;
  instance_id: string;
  definition_name: string;
  from_state?: string;
  to_state?: string;
  trigger?: string;
  message?: string;
  deployed_nodes?: number;
  failed_nodes?: number;
  target_nodes?: number;
  final_status?: string;
  timestamp: string;
}

export interface Overview {
  node_count: number;
  messages_in: number;
  messages_out: number;
  packets_in: number;
  packets_out: number;
  bot_name: string;
}

export interface DashboardNode {
  node_id: string;
  short_name: string;
  long_name: string;
  last_seen: number;
  last_rf_seen: number | null;
  latitude: number | null;
  longitude: number | null;
  via_mqtt: boolean;
  last_hop: number | null;
  min_hop: number | null;
  avg_hop: number | null;
  hop_samples: number;
}

export interface ThroughputBucket {
  hour: string;
  incoming: number;
  outgoing: number;
}

export interface DistributionBucket {
  label: string;
  count: number;
}

export interface QueueDepth {
  depth: number;
}

export interface TracerouteRequester {
  node_id: string;
  short_name: string;
  long_name: string;
  request_count: number;
  last_request: number;
  via_mqtt: boolean;
}

export interface TracerouteEventRow {
  timestamp: number;
  from_node: string;
  from_short_name: string;
  from_long_name: string;
  to_node: string;
  to_short_name: string;
  to_long_name: string;
  via_mqtt: boolean;
  hop_count: number | null;
  hop_start: number | null;
  rssi: number | null;
  snr: number | null;
}

export interface TracerouteDestinationRow {
  destination_node: string;
  destination_short_name: string;
  destination_long_name: string;
  requests: number;
  unique_requesters: number;
  last_seen: number;
  rf_count: number;
  mqtt_count: number;
  avg_hops: number | null;
}

export interface TracerouteSessionHop {
  direction: string;
  hop_index: number;
  node_id: string;
  short_name: string | null;
  long_name: string | null;
}

export interface TracerouteSessionRow {
  id: number;
  trace_key: string;
  first_seen: number;
  last_seen: number;
  src_node: string;
  src_short_name: string | null;
  src_long_name: string | null;
  dst_node: string | null;
  dst_short_name: string | null;
  dst_long_name: string | null;
  via_mqtt: boolean;
  request_hops: number | null;
  request_hop_start: number | null;
  response_hops: number | null;
  response_hop_start: number | null;
  status: string;
  sample_count: number;
  hops: TracerouteSessionHop[];
}

export type MqttFilterValue = "all" | "local" | "mqtt_only";

export type HoursValue = 24 | 72 | 168 | 720 | 2160 | 8760 | 0;

export type PacketTypeFilter =
  | "all"
  | "text"
  | "position"
  | "telemetry"
  | "traceroute"
  | "other";

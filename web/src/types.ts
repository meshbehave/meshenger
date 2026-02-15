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

export type MqttFilterValue = 'all' | 'local' | 'mqtt_only';

export type HoursValue = 24 | 72 | 168 | 720 | 2160 | 8760 | 0;

export type PacketTypeFilter = 'all' | 'text' | 'position' | 'telemetry' | 'other';

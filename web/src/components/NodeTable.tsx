import { useState } from "react";
import type { DashboardNode } from "../types";

interface Props {
  nodes: DashboardNode[] | null;
}

type SortKey =
  | "node_id"
  | "long_name"
  | "last_seen"
  | "last_rf_seen"
  | "via_mqtt"
  | "last_hop"
  | "hop_samples";

function formatAgo(timestamp: number): string {
  const secs = Math.floor(Date.now() / 1000) - timestamp;
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function SourceBadge({ viaMqtt }: { viaMqtt: boolean }) {
  return viaMqtt ? (
    <span className="px-1.5 py-0.5 rounded text-xs font-medium bg-amber-900/50 text-amber-300">
      MQTT
    </span>
  ) : (
    <span className="px-1.5 py-0.5 rounded text-xs font-medium bg-emerald-900/50 text-emerald-300">
      RF
    </span>
  );
}

function formatHopSummary(
  lastHop: number | null,
  avgHop: number | null,
  minHop: number | null,
): string {
  if (lastHop == null && avgHop == null && minHop == null) return "—";
  const parts: string[] = [];
  if (lastHop != null) parts.push(`last ${lastHop}`);
  if (avgHop != null) parts.push(`avg ${avgHop.toFixed(1)}`);
  if (minHop != null) parts.push(`min ${minHop}`);
  return parts.join(" / ");
}

export function NodeTable({ nodes }: Props) {
  const [sortKey, setSortKey] = useState<SortKey>("last_seen");
  const [sortAsc, setSortAsc] = useState(false);

  if (!nodes || nodes.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
        <h3 className="text-sm font-medium text-slate-400 mb-3">Nodes</h3>
        <span className="text-slate-500">No nodes seen</span>
      </div>
    );
  }

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortAsc(!sortAsc);
    } else {
      setSortKey(key);
      setSortAsc(key !== "last_seen");
    }
  };

  const sorted = [...nodes].sort((a, b) => {
    if (sortKey === "via_mqtt") {
      const va = a.via_mqtt ? 1 : 0;
      const vb = b.via_mqtt ? 1 : 0;
      return sortAsc ? va - vb : vb - va;
    }
    if (sortKey === "last_hop") {
      const va = a.last_hop ?? -1;
      const vb = b.last_hop ?? -1;
      return sortAsc ? va - vb : vb - va;
    }
    if (sortKey === "last_rf_seen") {
      const va = a.last_rf_seen ?? 0;
      const vb = b.last_rf_seen ?? 0;
      return sortAsc ? va - vb : vb - va;
    }
    if (sortKey === "hop_samples") {
      return sortAsc
        ? a.hop_samples - b.hop_samples
        : b.hop_samples - a.hop_samples;
    }
    const va = a[sortKey];
    const vb = b[sortKey];
    if (typeof va === "string" && typeof vb === "string") {
      return sortAsc ? va.localeCompare(vb) : vb.localeCompare(va);
    }
    return sortAsc
      ? (va as number) - (vb as number)
      : (vb as number) - (va as number);
  });

  const arrow = (key: SortKey) =>
    sortKey === key ? (sortAsc ? " ^" : " v") : "";

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 overflow-x-auto">
      <h3 className="text-sm font-medium text-slate-400 mb-3">
        Nodes ({nodes.length})
      </h3>
      <table className="w-full text-sm">
        <thead>
          <tr className="text-slate-400 border-b border-slate-700">
            <th
              className="text-left py-2 px-2 cursor-pointer"
              onClick={() => handleSort("node_id")}
            >
              ID{arrow("node_id")}
            </th>
            <th
              className="text-left py-2 px-2 cursor-pointer"
              onClick={() => handleSort("long_name")}
            >
              Name{arrow("long_name")}
            </th>
            <th
              className="text-left py-2 px-2 cursor-pointer"
              onClick={() => handleSort("via_mqtt")}
            >
              Source{arrow("via_mqtt")}
            </th>
            <th
              className="text-left py-2 px-2 cursor-pointer"
              onClick={() => handleSort("last_seen")}
            >
              Last Seen{arrow("last_seen")}
            </th>
            <th
              className="text-left py-2 px-2 cursor-pointer"
              onClick={() => handleSort("last_rf_seen")}
            >
              Last RF{arrow("last_rf_seen")}
            </th>
            <th
              className="text-left py-2 px-2 cursor-pointer"
              onClick={() => handleSort("last_hop")}
            >
              Hops{arrow("last_hop")}
            </th>
            <th
              className="text-left py-2 px-2 cursor-pointer"
              onClick={() => handleSort("hop_samples")}
            >
              Samples{arrow("hop_samples")}
            </th>
            <th className="text-left py-2 px-2">Position</th>
          </tr>
        </thead>
        <tbody>
          {sorted.map((node) => (
            <tr
              key={node.node_id}
              className="border-b border-slate-700/50 hover:bg-slate-700/30"
            >
              <td className="py-2 px-2 font-mono text-xs">{node.node_id}</td>
              <td className="py-2 px-2">
                {node.long_name || node.short_name || "—"}
              </td>
              <td className="py-2 px-2">
                <SourceBadge viaMqtt={node.via_mqtt} />
              </td>
              <td className="py-2 px-2 text-slate-400">
                {formatAgo(node.last_seen)}
              </td>
              <td className="py-2 px-2 text-slate-400">
                {node.last_rf_seen != null ? formatAgo(node.last_rf_seen) : "—"}
              </td>
              <td className="py-2 px-2 text-slate-400">
                {formatHopSummary(node.last_hop, node.avg_hop, node.min_hop)}
              </td>
              <td className="py-2 px-2 text-slate-400">{node.hop_samples}</td>
              <td className="py-2 px-2 text-slate-400">
                {node.latitude != null && node.longitude != null
                  ? `${node.latitude.toFixed(4)}, ${node.longitude.toFixed(4)}`
                  : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

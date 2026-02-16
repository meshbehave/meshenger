import type { TracerouteRequester } from "../types";

interface Props {
  rows: TracerouteRequester[] | null;
}

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

export function TracerouteRequesterTable({ rows }: Props) {
  if (!rows || rows.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
        <h3 className="text-sm font-medium text-slate-400 mb-3">
          Traceroute Requests To Me
        </h3>
        <span className="text-slate-500">
          No incoming traceroute requests in this time range.
        </span>
      </div>
    );
  }

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 overflow-x-auto">
      <h3 className="text-sm font-medium text-slate-400 mb-3">
        Traceroute Requests To Me ({rows.length})
      </h3>
      <table className="w-full text-sm">
        <thead>
          <tr className="text-slate-400 border-b border-slate-700">
            <th className="text-left py-2 px-2">Requester</th>
            <th className="text-left py-2 px-2">Node ID</th>
            <th className="text-left py-2 px-2">Source</th>
            <th className="text-left py-2 px-2">Requests</th>
            <th className="text-left py-2 px-2">Last Request</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.node_id}
              className="border-b border-slate-700/50 hover:bg-slate-700/30"
            >
              <td className="py-2 px-2">
                {row.long_name || row.short_name || "unknown"}
              </td>
              <td className="py-2 px-2 font-mono text-xs">{row.node_id}</td>
              <td className="py-2 px-2">
                <SourceBadge viaMqtt={row.via_mqtt} />
              </td>
              <td className="py-2 px-2">{row.request_count}</td>
              <td className="py-2 px-2 text-slate-400">
                {formatAgo(row.last_request)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

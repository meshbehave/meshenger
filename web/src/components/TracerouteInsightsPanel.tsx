import { useMemo, useState } from "react";
import type {
  HopsToMeRow,
  TracerouteSessionDetail,
  TracerouteSessionHopRow,
  TracerouteSessionRow,
} from "../types";

interface Props {
  hopsToMe: HopsToMeRow[] | null;
  sessions: TracerouteSessionRow[] | null;
  selectedSession: TracerouteSessionDetail | null;
  onSelectSession: (sessionId: number) => void;
}

type Tab = "hops" | "sessions";

function formatAgo(timestamp: number): string {
  const secs = Math.floor(Date.now() / 1000) - timestamp;
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function tabClass(active: boolean): string {
  return active
    ? "px-3 py-1.5 rounded-md text-xs font-semibold bg-slate-700 text-slate-100"
    : "px-3 py-1.5 rounded-md text-xs font-semibold text-slate-400 hover:text-slate-200 hover:bg-slate-700/60";
}

function statusClass(status: string): string {
  if (status === "complete") {
    return "px-2 py-0.5 rounded text-xs font-medium bg-emerald-900/50 text-emerald-300";
  }
  if (status === "partial") {
    return "px-2 py-0.5 rounded text-xs font-medium bg-amber-900/50 text-amber-300";
  }
  return "px-2 py-0.5 rounded text-xs font-medium bg-slate-700 text-slate-300";
}

function renderHopTimeline(hops: TracerouteSessionHopRow[]): string {
  if (hops.length === 0) {
    return "No path hops decoded";
  }
  return hops.map((h) => h.node_id).join(" -> ");
}

export function TracerouteInsightsPanel({
  hopsToMe,
  sessions,
  selectedSession,
  onSelectSession,
}: Props) {
  const [tab, setTab] = useState<Tab>("hops");
  const hopRows = useMemo(() => hopsToMe ?? [], [hopsToMe]);
  const sessionRows = useMemo(() => sessions ?? [], [sessions]);

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 overflow-x-auto">
      <div className="flex items-center justify-between mb-3 gap-3 flex-wrap">
        <h3 className="text-sm font-medium text-slate-400">
          Traceroute Insights
        </h3>
        <div className="bg-slate-900/70 rounded-lg p-1 flex gap-1">
          <button
            className={tabClass(tab === "hops")}
            onClick={() => setTab("hops")}
          >
            Hops To Me ({hopRows.length})
          </button>
          <button
            className={tabClass(tab === "sessions")}
            onClick={() => setTab("sessions")}
          >
            Sessions ({sessionRows.length})
          </button>
        </div>
      </div>

      {tab === "hops" ? (
        hopRows.length === 0 ? (
          <span className="text-slate-500">
            No traceroute hops-to-me samples in this time range.
          </span>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-slate-400 border-b border-slate-700">
                <th className="text-left py-2 px-2">Source</th>
                <th className="text-left py-2 px-2">Last</th>
                <th className="text-left py-2 px-2">Min/Avg/Max</th>
                <th className="text-left py-2 px-2">Samples</th>
                <th className="text-left py-2 px-2">Last Seen</th>
                <th className="text-left py-2 px-2">RF</th>
                <th className="text-left py-2 px-2">MQTT</th>
              </tr>
            </thead>
            <tbody>
              {hopRows.map((row) => (
                <tr
                  key={row.source_node}
                  className="border-b border-slate-700/50 hover:bg-slate-700/30"
                >
                  <td className="py-2 px-2">
                    {(row.source_long_name ||
                      row.source_short_name ||
                      "unknown") +
                      " " +
                      row.source_node}
                  </td>
                  <td className="py-2 px-2 text-slate-300">
                    {row.last_hops != null ? row.last_hops : "-"}
                  </td>
                  <td className="py-2 px-2 text-slate-400">
                    {row.min_hops != null &&
                    row.avg_hops != null &&
                    row.max_hops != null
                      ? `${row.min_hops}/${row.avg_hops.toFixed(2)}/${row.max_hops}`
                      : "-"}
                  </td>
                  <td className="py-2 px-2">{row.samples}</td>
                  <td className="py-2 px-2 text-slate-400">
                    {formatAgo(row.last_seen)}
                  </td>
                  <td className="py-2 px-2">{row.rf_count}</td>
                  <td className="py-2 px-2">{row.mqtt_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )
      ) : sessionRows.length === 0 ? (
        <span className="text-slate-500">
          No traceroute sessions in this time range.
        </span>
      ) : (
        <div className="space-y-3">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-slate-400 border-b border-slate-700">
                <th className="text-left py-2 px-2">From</th>
                <th className="text-left py-2 px-2">To</th>
                <th className="text-left py-2 px-2">Status</th>
                <th className="text-left py-2 px-2">Request</th>
                <th className="text-left py-2 px-2">Response</th>
                <th className="text-left py-2 px-2">Samples</th>
                <th className="text-left py-2 px-2">Last Seen</th>
              </tr>
            </thead>
            <tbody>
              {sessionRows.map((row) => (
                <tr
                  key={row.id}
                  className="border-b border-slate-700/50 hover:bg-slate-700/30 cursor-pointer"
                  onClick={() => onSelectSession(row.id)}
                >
                  <td className="py-2 px-2">
                    {(row.src_long_name || row.src_short_name || "unknown") +
                      " " +
                      row.src_node}
                  </td>
                  <td className="py-2 px-2">
                    {row.dst_node === "broadcast"
                      ? "broadcast"
                      : (row.dst_long_name || row.dst_short_name || "unknown") +
                        " " +
                        row.dst_node}
                  </td>
                  <td className="py-2 px-2">
                    <span className={statusClass(row.status)}>
                      {row.status}
                    </span>
                  </td>
                  <td className="py-2 px-2 text-slate-400">
                    {row.request_hops != null && row.request_hop_start != null
                      ? `${row.request_hops}/${row.request_hop_start}`
                      : "-"}
                  </td>
                  <td className="py-2 px-2 text-slate-400">
                    {row.response_hops != null && row.response_hop_start != null
                      ? `${row.response_hops}/${row.response_hop_start}`
                      : "-"}
                  </td>
                  <td className="py-2 px-2">{row.sample_count}</td>
                  <td className="py-2 px-2 text-slate-400">
                    {formatAgo(row.last_seen)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {selectedSession ? (
            <div className="rounded-md border border-slate-700 bg-slate-900/40 p-3 space-y-2">
              <div className="text-sm text-slate-300">
                Selected Session #{selectedSession.session.id} (
                {selectedSession.session.trace_key})
              </div>
              <div className="text-xs text-slate-400">
                Request path:{" "}
                {renderHopTimeline(
                  selectedSession.hops.filter((h) => h.direction === "request"),
                )}
              </div>
              <div className="text-xs text-slate-400">
                Response path:{" "}
                {renderHopTimeline(
                  selectedSession.hops.filter(
                    (h) => h.direction === "response",
                  ),
                )}
              </div>
            </div>
          ) : (
            <div className="text-xs text-slate-500">
              Select a session row to inspect decoded path hops.
            </div>
          )}
        </div>
      )}
    </div>
  );
}

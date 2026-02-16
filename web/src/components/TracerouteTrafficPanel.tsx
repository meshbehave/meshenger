import { useMemo, useState } from "react";
import type { TracerouteDestinationRow, TracerouteEventRow } from "../types";

interface Props {
  events: TracerouteEventRow[] | null;
  destinations: TracerouteDestinationRow[] | null;
}

type Tab = "events" | "destinations";

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

function tabClass(active: boolean): string {
  return active
    ? "px-3 py-1.5 rounded-md text-xs font-semibold bg-slate-700 text-slate-100"
    : "px-3 py-1.5 rounded-md text-xs font-semibold text-slate-400 hover:text-slate-200 hover:bg-slate-700/60";
}

export function TracerouteTrafficPanel({ events, destinations }: Props) {
  const [tab, setTab] = useState<Tab>("events");

  const eventRows = useMemo(() => events ?? [], [events]);
  const destinationRows = useMemo(() => destinations ?? [], [destinations]);

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 overflow-x-auto">
      <div className="flex items-center justify-between mb-3 gap-3 flex-wrap">
        <h3 className="text-sm font-medium text-slate-400">
          Traceroute Traffic
        </h3>
        <div className="bg-slate-900/70 rounded-lg p-1 flex gap-1">
          <button
            className={tabClass(tab === "events")}
            onClick={() => setTab("events")}
          >
            Events ({eventRows.length})
          </button>
          <button
            className={tabClass(tab === "destinations")}
            onClick={() => setTab("destinations")}
          >
            Destinations ({destinationRows.length})
          </button>
        </div>
      </div>

      {tab === "events" ? (
        eventRows.length === 0 ? (
          <span className="text-slate-500">
            No incoming traceroute events in this time range.
          </span>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-slate-400 border-b border-slate-700">
                <th className="text-left py-2 px-2">Time</th>
                <th className="text-left py-2 px-2">From</th>
                <th className="text-left py-2 px-2">To</th>
                <th className="text-left py-2 px-2">Source</th>
                <th className="text-left py-2 px-2">Hops</th>
                <th className="text-left py-2 px-2">RSSI</th>
                <th className="text-left py-2 px-2">SNR</th>
              </tr>
            </thead>
            <tbody>
              {eventRows.map((row) => (
                <tr
                  key={`${row.timestamp}-${row.from_node}-${row.to_node}`}
                  className="border-b border-slate-700/50 hover:bg-slate-700/30"
                >
                  <td className="py-2 px-2 text-slate-400">
                    {formatAgo(row.timestamp)}
                  </td>
                  <td className="py-2 px-2">
                    {(row.from_long_name || row.from_short_name || "unknown") +
                      " " +
                      row.from_node}
                  </td>
                  <td className="py-2 px-2">
                    {row.to_node === "broadcast"
                      ? "broadcast"
                      : (row.to_long_name || row.to_short_name || "unknown") +
                        " " +
                        row.to_node}
                  </td>
                  <td className="py-2 px-2">
                    <SourceBadge viaMqtt={row.via_mqtt} />
                  </td>
                  <td className="py-2 px-2 text-slate-400">
                    {row.hop_count != null && row.hop_start != null
                      ? `${row.hop_count}/${row.hop_start}`
                      : "-"}
                  </td>
                  <td className="py-2 px-2 text-slate-400">
                    {row.rssi != null ? row.rssi : "-"}
                  </td>
                  <td className="py-2 px-2 text-slate-400">
                    {row.snr != null ? row.snr.toFixed(1) : "-"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )
      ) : destinationRows.length === 0 ? (
        <span className="text-slate-500">
          No traceroute destinations in this time range.
        </span>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="text-slate-400 border-b border-slate-700">
              <th className="text-left py-2 px-2">Destination</th>
              <th className="text-left py-2 px-2">Requests</th>
              <th className="text-left py-2 px-2">Unique Requesters</th>
              <th className="text-left py-2 px-2">Last Seen</th>
              <th className="text-left py-2 px-2">RF</th>
              <th className="text-left py-2 px-2">MQTT</th>
              <th className="text-left py-2 px-2">Avg Hops</th>
            </tr>
          </thead>
          <tbody>
            {destinationRows.map((row) => (
              <tr
                key={row.destination_node}
                className="border-b border-slate-700/50 hover:bg-slate-700/30"
              >
                <td className="py-2 px-2">
                  {row.destination_node === "broadcast"
                    ? "broadcast"
                    : (row.destination_long_name ||
                        row.destination_short_name ||
                        "unknown") +
                      " " +
                      row.destination_node}
                </td>
                <td className="py-2 px-2">{row.requests}</td>
                <td className="py-2 px-2">{row.unique_requesters}</td>
                <td className="py-2 px-2 text-slate-400">
                  {formatAgo(row.last_seen)}
                </td>
                <td className="py-2 px-2">{row.rf_count}</td>
                <td className="py-2 px-2">{row.mqtt_count}</td>
                <td className="py-2 px-2 text-slate-400">
                  {row.avg_hops != null ? row.avg_hops.toFixed(2) : "-"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

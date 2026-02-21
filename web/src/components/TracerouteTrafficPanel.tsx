import { useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type {
  TracerouteDestinationRow,
  TracerouteEventRow,
  TracerouteSessionHop,
  TracerouteSessionRow,
} from "../types";
import { PaginationControls } from "./PaginationControls";
import { buildPathSegs, buildFullText } from "../utils/pathDisplay";

interface Props {
  events: TracerouteEventRow[] | null;
  destinations: TracerouteDestinationRow[] | null;
  sessions: TracerouteSessionRow[] | null;
}

type Tab = "events" | "destinations" | "sessions";

function formatAgo(timestamp: number): string {
  const secs = Math.floor(Date.now() / 1000) - timestamp;
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function Tooltip({
  content,
  children,
  position = "above",
}: {
  content: React.ReactNode;
  children: React.ReactNode;
  position?: "above" | "below";
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const [coords, setCoords] = useState<{
    top: number;
    left: number;
  } | null>(null);

  const show = () => {
    if (ref.current) {
      const r = ref.current.getBoundingClientRect();
      setCoords(
        position === "above"
          ? { top: r.top - 6, left: r.left }
          : { top: r.bottom + 6, left: r.left },
      );
    }
  };

  return (
    <span
      ref={ref}
      className="inline-block"
      onMouseEnter={show}
      onMouseLeave={() => setCoords(null)}
    >
      {children}
      {coords &&
        createPortal(
          <div
            className="pointer-events-none fixed z-50"
            style={{
              top: coords.top,
              left: coords.left,
              transform: position === "above" ? "translateY(-100%)" : undefined,
            }}
          >
            {content}
          </div>,
          document.body,
        )}
    </span>
  );
}

function StatusMatrixTooltip() {
  const impossible = "text-slate-600 italic";
  const normal = "text-slate-300";
  return (
    <div className="w-max max-w-2xl rounded-lg border border-slate-600 bg-slate-900 p-3 shadow-xl">
      <table className="border-collapse text-xs">
        <thead>
          <tr>
            <th className="px-3 py-1 text-left font-normal text-slate-500"></th>
            <th className="px-3 py-1 font-semibold text-slate-400">observed</th>
            <th className="px-3 py-1 font-semibold text-slate-400">partial</th>
            <th className="px-3 py-1 font-semibold text-slate-400">complete</th>
          </tr>
        </thead>
        <tbody>
          <tr className="border-t border-slate-700">
            <td className="whitespace-nowrap px-3 py-1.5 text-xs font-medium text-slate-400">
              sent by us
            </td>
            <td className={`px-3 py-1.5 ${normal} max-w-[14rem]`}>
              Probe sent. No RouteReply received — target unreachable or reply
              hasn&apos;t arrived yet.
            </td>
            <td className={`px-3 py-1.5 ${impossible}`}>— impossible —</td>
            <td className={`px-3 py-1.5 ${normal} max-w-[14rem]`}>
              RouteReply received and correlated. Forward hops from route
              vector; return hops from reply RF metadata.
            </td>
          </tr>
          <tr className="border-t border-slate-700">
            <td className="whitespace-nowrap px-3 py-1.5 text-xs font-medium text-slate-400">
              not by us
            </td>
            <td className={`px-3 py-1.5 ${impossible}`}>— impossible —</td>
            <td className={`px-3 py-1.5 ${normal} max-w-[14rem]`}>
              One side seen. RouteRequest sniffed in transit — return path not
              yet observed, or reply didn&apos;t pass through our node.
            </td>
            <td className={`px-3 py-1.5 ${impossible}`}>
              — impossible — passive observations are capped at partial.
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

function NotAvailable({ reason }: { reason: string }) {
  return (
    <Tooltip
      position="above"
      content={
        <div className="w-52 rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-400 shadow-lg">
          {reason}
        </div>
      }
    >
      <span className="cursor-help text-slate-600 text-xs">∅ n/a</span>
    </Tooltip>
  );
}

function statusBlurb(status: string, byUs: boolean): string {
  if (status === "observed") {
    return byUs
      ? "Sent by us. No RouteReply received — target unreachable or reply hasn't arrived yet."
      : "Not expected in normal operation.";
  }
  if (status === "partial") {
    return byUs
      ? "Not expected in normal operation."
      : "Not sent by us. RouteRequest sniffed in transit — return path not yet observed, or reply didn't pass through our node.";
  }
  if (status === "complete") {
    return byUs
      ? "Sent by us. RouteReply received and correlated. Forward hops from route vector; return hops from reply RF metadata."
      : "Unexpected — passive observations are capped at partial.";
  }
  return "";
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

function StatusBadge({
  status,
  traceKey,
}: {
  status: string;
  traceKey: string;
}) {
  const cls =
    status === "complete"
      ? "bg-emerald-900/50 text-emerald-300"
      : status === "partial"
        ? "bg-amber-900/50 text-amber-300"
        : "bg-slate-700 text-slate-400";
  const byUs = traceKey.startsWith("req:");
  const blurb = statusBlurb(status, byUs);
  return (
    <Tooltip
      position="above"
      content={
        <div className="w-64 rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 shadow-lg">
          <span className="font-medium text-slate-400">
            {byUs ? "sent by us" : "not by us"}
          </span>
          <span className="mx-1 text-slate-600">·</span>
          {blurb}
        </div>
      }
    >
      <span className={`px-1.5 py-0.5 rounded text-xs font-medium ${cls}`}>
        {status}
      </span>
    </Tooltip>
  );
}

function tabClass(active: boolean): string {
  return active
    ? "px-3 py-1.5 rounded-md text-xs font-semibold bg-slate-700 text-slate-100"
    : "px-3 py-1.5 rounded-md text-xs font-semibold text-slate-400 hover:text-slate-200 hover:bg-slate-700/60";
}

function PathDisplay({
  hops,
  srcName,
  srcNode,
  dstName,
  dstNode,
  status,
}: {
  hops: TracerouteSessionHop[];
  srcName: string | null;
  srcNode: string;
  dstName: string | null;
  dstNode: string | null;
  status: string;
}) {
  const label = (name: string | null, id: string) => name || id;
  const byHopIdx = (a: TracerouteSessionHop, b: TracerouteSessionHop) =>
    a.hop_index - b.hop_index;
  const reqHops = [...hops]
    .filter((h) => h.direction === "request")
    .sort(byHopIdx)
    .map((h) => label(h.short_name, h.node_id));
  const resHops = [...hops]
    .filter((h) => h.direction === "response")
    .sort(byHopIdx)
    .map((h) => label(h.short_name, h.node_id));

  const srcLabel = label(srcName, srcNode);
  const dstLabel = dstNode ? label(dstName, dstNode) : "?";
  const isComplete = status === "complete";

  const fullText = buildFullText(reqHops, resHops, srcLabel, dstLabel);
  const { segs, didTruncate } = buildPathSegs(
    reqHops,
    resHops,
    srcLabel,
    dstLabel,
    isComplete,
  );

  const inner = (
    <span>
      {segs.map((seg, i) => (
        <span key={i}>
          {i > 0 && <span className="text-slate-500"> → </span>}
          <span className={seg.dim ? "text-slate-500" : ""}>{seg.text}</span>
        </span>
      ))}
    </span>
  );

  if (!didTruncate) return inner;

  return (
    <Tooltip
      position="above"
      content={
        <div className="max-w-xs rounded-md border border-slate-600 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 shadow-lg break-words">
          {fullText}
        </div>
      }
    >
      {inner}
    </Tooltip>
  );
}

function NodeWithTraceKey({
  label,
  traceKey,
}: {
  label: string;
  traceKey: string;
}) {
  return (
    <Tooltip
      position="above"
      content={
        <div className="rounded-md border border-slate-600 bg-slate-900 px-2.5 py-2 text-xs shadow-lg space-y-1.5 max-w-xs">
          <div className="font-mono text-slate-300 break-all">{traceKey}</div>
          <div className="border-t border-slate-700 pt-1.5 space-y-0.5 text-slate-500">
            <div>
              <span className="font-medium text-slate-400">req:</span>… probe
              sent by this node
            </div>
            <div>
              <span className="font-medium text-slate-400">in:</span>… observed
              in transit
            </div>
          </div>
        </div>
      }
    >
      <span className="cursor-help underline decoration-dotted decoration-slate-600">
        {label}
      </span>
    </Tooltip>
  );
}

export function TracerouteTrafficPanel({
  events,
  destinations,
  sessions,
}: Props) {
  const [tab, setTab] = useState<Tab>("events");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(25);

  const eventRows = useMemo(() => events ?? [], [events]);
  const destinationRows = useMemo(() => destinations ?? [], [destinations]);
  const sessionRows = useMemo(() => sessions ?? [], [sessions]);
  const activeTotal =
    tab === "events"
      ? eventRows.length
      : tab === "destinations"
        ? destinationRows.length
        : sessionRows.length;

  const totalPages = Math.max(1, Math.ceil(activeTotal / pageSize));
  const safePage = Math.min(page, totalPages);

  const pagedEvents = useMemo(() => {
    const start = (safePage - 1) * pageSize;
    return eventRows.slice(start, start + pageSize);
  }, [eventRows, pageSize, safePage]);

  const pagedDestinations = useMemo(() => {
    const start = (safePage - 1) * pageSize;
    return destinationRows.slice(start, start + pageSize);
  }, [destinationRows, pageSize, safePage]);

  const pagedSessions = useMemo(() => {
    const start = (safePage - 1) * pageSize;
    return sessionRows.slice(start, start + pageSize);
  }, [sessionRows, pageSize, safePage]);

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
      <div className="flex items-center justify-between mb-3 gap-3 flex-wrap">
        <h3 className="text-sm font-medium text-slate-400">
          Traceroute Traffic
        </h3>
        <div className="bg-slate-900/70 rounded-lg p-1 flex gap-1">
          <button
            className={tabClass(tab === "events")}
            onClick={() => {
              setTab("events");
              setPage(1);
            }}
          >
            Events ({eventRows.length})
          </button>
          <button
            className={tabClass(tab === "destinations")}
            onClick={() => {
              setTab("destinations");
              setPage(1);
            }}
          >
            Destinations ({destinationRows.length})
          </button>
          <button
            className={tabClass(tab === "sessions")}
            onClick={() => {
              setTab("sessions");
              setPage(1);
            }}
          >
            Sessions ({sessionRows.length})
          </button>
        </div>
      </div>

      {tab === "events" ? (
        eventRows.length === 0 ? (
          <span className="text-slate-500">
            No incoming traceroute events in this time range.
          </span>
        ) : (
          <>
            <div className="overflow-x-auto">
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
                  {pagedEvents.map((row) => (
                    <tr
                      key={`${row.timestamp}-${row.from_node}-${row.to_node}`}
                      className="border-b border-slate-700/50 hover:bg-slate-700/30"
                    >
                      <td className="py-2 px-2 text-slate-400">
                        {formatAgo(row.timestamp)}
                      </td>
                      <td className="py-2 px-2">
                        {(row.from_long_name ||
                          row.from_short_name ||
                          "unknown") +
                          " " +
                          row.from_node}
                      </td>
                      <td className="py-2 px-2">
                        {row.to_node === "broadcast"
                          ? "broadcast"
                          : (row.to_long_name ||
                              row.to_short_name ||
                              "unknown") +
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
            </div>
            <PaginationControls
              page={safePage}
              pageSize={pageSize}
              total={eventRows.length}
              onPageChange={setPage}
              onPageSizeChange={(value) => {
                setPageSize(value);
                setPage(1);
              }}
            />
          </>
        )
      ) : tab === "destinations" ? (
        destinationRows.length === 0 ? (
          <span className="text-slate-500">
            No traceroute destinations in this time range.
          </span>
        ) : (
          <>
            <div className="overflow-x-auto">
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
                  {pagedDestinations.map((row) => (
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
            </div>
            <PaginationControls
              page={safePage}
              pageSize={pageSize}
              total={destinationRows.length}
              onPageChange={setPage}
              onPageSizeChange={(value) => {
                setPageSize(value);
                setPage(1);
              }}
            />
          </>
        )
      ) : sessionRows.length === 0 ? (
        <span className="text-slate-500">
          No traceroute sessions in this time range.
        </span>
      ) : (
        <>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-slate-400 border-b border-slate-700">
                  <th className="text-left py-2 px-2">Time</th>
                  <th className="text-left py-2 px-2">From</th>
                  <th className="text-left py-2 px-2">To</th>
                  <th className="text-left py-2 px-2">Path</th>
                  <th className="text-left py-2 px-2">
                    <Tooltip position="below" content={<StatusMatrixTooltip />}>
                      <span className="cursor-help underline decoration-dotted decoration-slate-500">
                        Status
                      </span>
                    </Tooltip>
                  </th>
                  <th className="text-left py-2 px-2">
                    <Tooltip
                      position="below"
                      content={
                        <div className="w-64 rounded-md border border-slate-600 bg-slate-900 px-2.5 py-2 text-xs text-slate-300 shadow-lg space-y-1.5">
                          <div>
                            <span className="font-medium text-slate-400">
                              sent by us
                            </span>
                            <span className="mx-1 text-slate-600">·</span>
                            Always —. We are the sender; outgoing packets carry
                            no RF metadata back to us.
                          </div>
                          <div>
                            <span className="font-medium text-slate-400">
                              not by us
                            </span>
                            <span className="mx-1 text-slate-600">·</span>
                            RF hop count of the sniffed packet as it passed
                            through our node (hops used / hop start).
                          </div>
                        </div>
                      }
                    >
                      <span className="cursor-help underline decoration-dotted decoration-slate-500">
                        Request
                      </span>
                    </Tooltip>
                  </th>
                  <th className="text-left py-2 px-2">
                    <Tooltip
                      position="below"
                      content={
                        <div className="w-64 rounded-md border border-slate-600 bg-slate-900 px-2.5 py-2 text-xs text-slate-300 shadow-lg space-y-1.5">
                          <div>
                            <span className="font-medium text-slate-400">
                              sent by us
                            </span>
                            <span className="mx-1 text-slate-600">·</span>
                            RF hop count of the RouteReply addressed back to us
                            (hops used / hop start).
                          </div>
                          <div>
                            <span className="font-medium text-slate-400">
                              not by us
                            </span>
                            <span className="mx-1 text-slate-600">·</span>
                            Always —. The RouteReply flies through us as a relay
                            mid-flight, not as its destination. The final return
                            hop count is only observable by the original sender.
                          </div>
                        </div>
                      }
                    >
                      <span className="cursor-help underline decoration-dotted decoration-slate-500">
                        Response
                      </span>
                    </Tooltip>
                  </th>
                  <th className="text-left py-2 px-2">Src</th>
                  <th className="text-left py-2 px-2">Samples</th>
                </tr>
              </thead>
              <tbody>
                {pagedSessions.map((row) => (
                  <tr
                    key={row.id}
                    className="border-b border-slate-700/50 hover:bg-slate-700/30"
                  >
                    <td className="py-2 px-2 text-slate-400">
                      {formatAgo(row.last_seen)}
                    </td>
                    <td className="py-2 px-2">
                      <NodeWithTraceKey
                        label={row.src_short_name || row.src_node}
                        traceKey={row.trace_key}
                      />
                    </td>
                    <td className="py-2 px-2">
                      <NodeWithTraceKey
                        label={
                          row.dst_node
                            ? row.dst_short_name || row.dst_node
                            : "?"
                        }
                        traceKey={row.trace_key}
                      />
                    </td>
                    <td className="py-2 px-2 text-slate-300">
                      <PathDisplay
                        hops={row.hops}
                        srcName={row.src_short_name}
                        srcNode={row.src_node}
                        dstName={row.dst_short_name}
                        dstNode={row.dst_node}
                        status={row.status}
                      />
                    </td>
                    <td className="py-2 px-2">
                      <StatusBadge
                        status={row.status}
                        traceKey={row.trace_key}
                      />
                    </td>
                    <td className="py-2 px-2 text-slate-400">
                      {row.request_hops != null ? (
                        row.request_hop_start != null ? (
                          `${row.request_hops}/${row.request_hop_start}`
                        ) : (
                          `${row.request_hops}`
                        )
                      ) : (
                        <NotAvailable
                          reason={
                            row.trace_key.startsWith("req:")
                              ? "Sent by us — outgoing packets carry no RF metadata back to the sender."
                              : "Not observable — packet was sniffed mid-flight, not addressed to us."
                          }
                        />
                      )}
                    </td>
                    <td className="py-2 px-2 text-slate-400">
                      {row.response_hops != null ? (
                        row.response_hop_start != null ? (
                          `${row.response_hops}/${row.response_hop_start}`
                        ) : (
                          `${row.response_hops}`
                        )
                      ) : (
                        <NotAvailable
                          reason={
                            row.trace_key.startsWith("req:")
                              ? "No RouteReply received yet for this probe."
                              : "RouteReply flew through us as a relay mid-flight — final return hop count is only observable by the original sender."
                          }
                        />
                      )}
                    </td>
                    <td className="py-2 px-2">
                      <SourceBadge viaMqtt={row.via_mqtt} />
                    </td>
                    <td className="py-2 px-2 text-slate-400">
                      {row.sample_count}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <PaginationControls
            page={safePage}
            pageSize={pageSize}
            total={sessionRows.length}
            onPageChange={setPage}
            onPageSizeChange={(value) => {
              setPageSize(value);
              setPage(1);
            }}
          />
        </>
      )}
    </div>
  );
}

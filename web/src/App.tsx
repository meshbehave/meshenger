import { useState, useEffect, useCallback, useMemo } from "react";
import type {
  Overview,
  DashboardNode,
  ThroughputBucket,
  DistributionBucket,
  QueueDepth,
  TracerouteRequester,
  MqttFilterValue,
  HoursValue,
  PacketTypeFilter,
} from "./types";
import { MqttFilter } from "./components/MqttFilter";
import { TimeRangeSelector } from "./components/TimeRangeSelector";
import { OverviewCards } from "./components/OverviewCards";
import { ThroughputChart } from "./components/ThroughputChart";
import { PacketThroughputChart } from "./components/PacketThroughputChart";
import { RssiChart } from "./components/RssiChart";
import { SnrChart } from "./components/SnrChart";
import { HopChart } from "./components/HopChart";
import { NodeTable } from "./components/NodeTable";
import { NodeMap } from "./components/NodeMap";
import { TracerouteRequesterTable } from "./components/TracerouteRequesterTable";

const REFRESH_INTERVAL = 30_000;

function App() {
  const [mqtt, setMqtt] = useState<MqttFilterValue>("all");
  const [hours, setHours] = useState<HoursValue>(24);
  const [packetFilter, setPacketFilter] = useState<PacketTypeFilter>("all");
  const [overview, setOverview] = useState<Overview | null>(null);
  const [nodes, setNodes] = useState<DashboardNode[] | null>(null);
  const [throughput, setThroughput] = useState<ThroughputBucket[] | null>(null);
  const [packetThroughput, setPacketThroughput] = useState<
    ThroughputBucket[] | null
  >(null);
  const [rssi, setRssi] = useState<DistributionBucket[] | null>(null);
  const [snr, setSnr] = useState<DistributionBucket[] | null>(null);
  const [hops, setHops] = useState<DistributionBucket[] | null>(null);
  const [queue, setQueue] = useState<QueueDepth | null>(null);
  const [tracerouteRequesters, setTracerouteRequesters] = useState<
    TracerouteRequester[] | null
  >(null);

  const params = useMemo(() => {
    const p = new URLSearchParams({ mqtt });
    if (hours > 0) p.set("hours", String(hours));
    else p.set("hours", "0");
    return p;
  }, [mqtt, hours]);

  const fetchAll = useCallback(async () => {
    const p = params.toString();
    const [ov, nd, tp, pt, rs, sn, hp, qu, tr] = await Promise.all([
      fetch(`/api/overview?${p}`).then((r) => (r.ok ? r.json() : null)),
      fetch(`/api/nodes?${p}`).then((r) => (r.ok ? r.json() : null)),
      fetch(`/api/throughput?${p}`).then((r) => (r.ok ? r.json() : null)),
      fetch(
        `/api/packet-throughput?${p}${packetFilter !== "all" ? `&types=${packetFilter}` : ""}`,
      ).then((r) => (r.ok ? r.json() : null)),
      fetch(`/api/rssi?${p}`).then((r) => (r.ok ? r.json() : null)),
      fetch(`/api/snr?${p}`).then((r) => (r.ok ? r.json() : null)),
      fetch(`/api/hops?${p}`).then((r) => (r.ok ? r.json() : null)),
      fetch("/api/queue").then((r) => (r.ok ? r.json() : null)),
      fetch(`/api/traceroute-requesters?${p}`).then((r) =>
        r.ok ? r.json() : null,
      ),
    ]);
    setOverview(ov);
    setNodes(nd);
    setThroughput(tp);
    setPacketThroughput(pt);
    setRssi(rs);
    setSnr(sn);
    setHops(hp);
    setQueue(qu);
    setTracerouteRequesters(tr);
  }, [params, packetFilter]);

  useEffect(() => {
    // Schedule initial fetch on next tick to avoid sync setState in effect body.
    const initialFetchId = setTimeout(() => {
      void fetchAll();
    }, 0);

    // Use SSE for real-time updates, with polling as fallback
    const es = new EventSource("/api/events");
    es.addEventListener("refresh", () => fetchAll());

    // Fallback polling in case SSE disconnects silently
    const id = setInterval(fetchAll, REFRESH_INTERVAL);

    return () => {
      es.close();
      clearTimeout(initialFetchId);
      clearInterval(id);
    };
  }, [fetchAll]);

  return (
    <div className="min-h-screen bg-slate-900 text-slate-200">
      <header className="border-b border-slate-700 px-6 py-4 flex items-center justify-between flex-wrap gap-3">
        <h1 className="text-xl font-bold">
          {overview?.bot_name ?? "Meshenger"} Dashboard
        </h1>
        <div className="flex items-center gap-3">
          <TimeRangeSelector value={hours} onChange={setHours} />
          <MqttFilter value={mqtt} onChange={setMqtt} />
        </div>
      </header>

      <main className="max-w-7xl mx-auto p-6 space-y-6">
        <OverviewCards overview={overview} queue={queue} hours={hours} />

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <ThroughputChart data={throughput} />
          <PacketThroughputChart
            data={packetThroughput}
            packetFilter={packetFilter}
            onPacketFilterChange={setPacketFilter}
          />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <RssiChart data={rssi} />
          <SnrChart data={snr} />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <HopChart data={hops} />
        </div>

        <TracerouteRequesterTable rows={tracerouteRequesters} />

        <NodeMap nodes={nodes} />

        <NodeTable nodes={nodes} />
      </main>
    </div>
  );
}

export default App;

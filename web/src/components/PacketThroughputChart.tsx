import { Line } from "react-chartjs-2";
import type { ThroughputBucket, PacketTypeFilter } from "../types";

interface Props {
  data: ThroughputBucket[] | null;
  packetFilter: PacketTypeFilter;
  onPacketFilterChange: (f: PacketTypeFilter) => void;
}

const filterOptions: { value: PacketTypeFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "text", label: "Text" },
  { value: "position", label: "Position" },
  { value: "telemetry", label: "Telemetry" },
  { value: "other", label: "Other" },
];

export function PacketThroughputChart({
  data,
  packetFilter,
  onPacketFilterChange,
}: Props) {
  if (!data || data.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 flex items-center justify-center h-64">
        <span className="text-slate-500">No packet throughput data</span>
      </div>
    );
  }

  const labels = data.map((b) => {
    const parts = b.hour.split(" ");
    return parts[1] || b.hour;
  });

  const chartData = {
    labels,
    datasets: [
      {
        label: "Incoming",
        data: data.map((b) => b.incoming),
        borderColor: "#8b5cf6",
        backgroundColor: "rgba(139, 92, 246, 0.1)",
        fill: true,
        tension: 0.3,
      },
      {
        label: "Outgoing",
        data: data.map((b) => b.outgoing),
        borderColor: "#f59e0b",
        backgroundColor: "rgba(245, 158, 11, 0.1)",
        fill: true,
        tension: 0.3,
      },
    ],
  };

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-medium text-slate-400">
          Packet Throughput
        </h3>
        <div className="flex gap-1">
          {filterOptions.map((opt) => (
            <button
              key={opt.value}
              onClick={() => onPacketFilterChange(opt.value)}
              className={`px-2 py-0.5 rounded text-xs font-medium transition-colors ${
                packetFilter === opt.value
                  ? "bg-violet-600 text-white"
                  : "bg-slate-700 text-slate-400 hover:bg-slate-600"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
      <Line
        data={chartData}
        options={{
          responsive: true,
          plugins: { legend: { labels: { color: "#94a3b8" } } },
          scales: {
            x: { ticks: { color: "#64748b" }, grid: { color: "#1e293b" } },
            y: {
              ticks: { color: "#64748b" },
              grid: { color: "#1e293b" },
              beginAtZero: true,
            },
          },
        }}
      />
    </div>
  );
}

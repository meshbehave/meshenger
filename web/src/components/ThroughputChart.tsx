import { Line } from "react-chartjs-2";
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Title,
  Tooltip,
  Legend,
  Filler,
} from "chart.js";
import type { ThroughputBucket } from "../types";

ChartJS.register(
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Title,
  Tooltip,
  Legend,
  Filler,
);

interface Props {
  data: ThroughputBucket[] | null;
}

export function ThroughputChart({ data }: Props) {
  if (!data || data.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 flex items-center justify-center h-64">
        <span className="text-slate-500">No throughput data</span>
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
        borderColor: "#3b82f6",
        backgroundColor: "rgba(59, 130, 246, 0.1)",
        fill: true,
        tension: 0.3,
      },
      {
        label: "Outgoing",
        data: data.map((b) => b.outgoing),
        borderColor: "#10b981",
        backgroundColor: "rgba(16, 185, 129, 0.1)",
        fill: true,
        tension: 0.3,
      },
    ],
  };

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
      <h3 className="text-sm font-medium text-slate-400 mb-3">
        Message Throughput
      </h3>
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

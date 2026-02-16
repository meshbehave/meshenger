import { Bar } from "react-chartjs-2";
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  BarElement,
  Tooltip,
} from "chart.js";
import type { DistributionBucket } from "../types";

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip);

interface Props {
  data: DistributionBucket[] | null;
}

export function SnrChart({ data }: Props) {
  if (!data || data.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 flex items-center justify-center h-64">
        <span className="text-slate-500">No SNR data</span>
      </div>
    );
  }

  const chartData = {
    labels: data.map((b) => b.label),
    datasets: [
      {
        label: "Messages",
        data: data.map((b) => b.count),
        backgroundColor: "#8b5cf6",
      },
    ],
  };

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
      <h3 className="text-sm font-medium text-slate-400 mb-3">
        SNR Distribution
      </h3>
      <Bar
        data={chartData}
        options={{
          responsive: true,
          plugins: { legend: { display: false } },
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

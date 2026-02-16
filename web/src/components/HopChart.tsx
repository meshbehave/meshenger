import { Doughnut } from "react-chartjs-2";
import { Chart as ChartJS, ArcElement, Tooltip, Legend } from "chart.js";
import type { DistributionBucket } from "../types";

ChartJS.register(ArcElement, Tooltip, Legend);

interface Props {
  data: DistributionBucket[] | null;
}

const COLORS = [
  "#3b82f6",
  "#10b981",
  "#f59e0b",
  "#ef4444",
  "#8b5cf6",
  "#ec4899",
  "#06b6d4",
];

export function HopChart({ data }: Props) {
  if (!data || data.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 flex items-center justify-center h-64">
        <span className="text-slate-500">No hop data</span>
      </div>
    );
  }

  const chartData = {
    labels: data.map((b) => b.label),
    datasets: [
      {
        data: data.map((b) => b.count),
        backgroundColor: data.map((_, i) => COLORS[i % COLORS.length]),
        borderColor: "#1e293b",
        borderWidth: 2,
      },
    ],
  };

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
      <h3 className="text-sm font-medium text-slate-400 mb-3">
        Hop Count Distribution
      </h3>
      <div className="flex justify-center">
        <div className="w-64 h-64">
          <Doughnut
            data={chartData}
            options={{
              responsive: true,
              maintainAspectRatio: false,
              plugins: {
                legend: { labels: { color: "#94a3b8" }, position: "bottom" },
              },
            }}
          />
        </div>
      </div>
    </div>
  );
}

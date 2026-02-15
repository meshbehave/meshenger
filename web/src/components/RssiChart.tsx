import { Bar } from 'react-chartjs-2';
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  BarElement,
  Title,
  Tooltip,
} from 'chart.js';
import type { DistributionBucket } from '../types';

ChartJS.register(CategoryScale, LinearScale, BarElement, Title, Tooltip);

interface Props {
  data: DistributionBucket[] | null;
}

export function RssiChart({ data }: Props) {
  if (!data || data.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4 border border-slate-700 flex items-center justify-center h-64">
        <span className="text-slate-500">No RSSI data</span>
      </div>
    );
  }

  const chartData = {
    labels: data.map((b) => b.label),
    datasets: [
      {
        label: 'Messages',
        data: data.map((b) => b.count),
        backgroundColor: '#f59e0b',
      },
    ],
  };

  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
      <h3 className="text-sm font-medium text-slate-400 mb-3">RSSI Distribution</h3>
      <Bar
        data={chartData}
        options={{
          responsive: true,
          plugins: { legend: { display: false } },
          scales: {
            x: { ticks: { color: '#64748b' }, grid: { color: '#1e293b' } },
            y: { ticks: { color: '#64748b' }, grid: { color: '#1e293b' }, beginAtZero: true },
          },
        }}
      />
    </div>
  );
}

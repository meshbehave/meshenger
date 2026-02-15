import type { Overview, QueueDepth, HoursValue } from '../types';

interface Props {
  overview: Overview | null;
  queue: QueueDepth | null;
  hours: HoursValue;
}

function Card({ title, value }: { title: string; value: string | number }) {
  return (
    <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
      <div className="text-slate-400 text-sm">{title}</div>
      <div className="text-2xl font-bold mt-1">{value}</div>
    </div>
  );
}

function hoursLabel(hours: HoursValue): string {
  if (hours === 0) return 'All';
  if (hours <= 24) return '24h';
  if (hours <= 72) return '3d';
  if (hours <= 168) return '7d';
  if (hours <= 720) return '30d';
  if (hours <= 2160) return '90d';
  return '365d';
}

export function OverviewCards({ overview, queue, hours }: Props) {
  const label = hoursLabel(hours);
  return (
    <div className="grid grid-cols-2 lg:grid-cols-3 gap-4">
      <Card title="Total Nodes" value={overview?.node_count ?? '—'} />
      <Card title={`Messages In (${label})`} value={overview?.messages_in ?? '—'} />
      <Card title={`Messages Out (${label})`} value={overview?.messages_out ?? '—'} />
      <Card title={`Packets In (${label})`} value={overview?.packets_in ?? '—'} />
      <Card title={`Packets Out (${label})`} value={overview?.packets_out ?? '—'} />
      <Card title="Queue Depth" value={queue?.depth ?? 0} />
    </div>
  );
}

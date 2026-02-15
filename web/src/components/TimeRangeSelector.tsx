import type { HoursValue } from '../types';

interface Props {
  value: HoursValue;
  onChange: (v: HoursValue) => void;
}

const options: { value: HoursValue; label: string }[] = [
  { value: 24, label: '1d' },
  { value: 72, label: '3d' },
  { value: 168, label: '7d' },
  { value: 720, label: '30d' },
  { value: 2160, label: '90d' },
  { value: 8760, label: '365d' },
  { value: 0, label: 'All' },
];

export function TimeRangeSelector({ value, onChange }: Props) {
  return (
    <div className="flex gap-1">
      {options.map((opt) => (
        <button
          key={opt.value}
          onClick={() => onChange(opt.value)}
          className={`px-3 py-1 rounded text-sm font-medium transition-colors ${
            value === opt.value
              ? 'bg-blue-600 text-white'
              : 'bg-slate-700 text-slate-300 hover:bg-slate-600'
          }`}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

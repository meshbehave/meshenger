import type { MqttFilterValue } from '../types';

interface Props {
  value: MqttFilterValue;
  onChange: (v: MqttFilterValue) => void;
}

const options: { value: MqttFilterValue; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'local', label: 'Local RF' },
  { value: 'mqtt_only', label: 'MQTT Only' },
];

export function MqttFilter({ value, onChange }: Props) {
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

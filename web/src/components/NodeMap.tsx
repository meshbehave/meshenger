import { useMemo } from 'react';
import { MapContainer, TileLayer, Marker, Popup } from 'react-leaflet';
import L from 'leaflet';
import 'leaflet/dist/leaflet.css';
import type { DashboardNode } from '../types';

// Fix default marker icons (leaflet CSS expects images in a specific path)
import markerIcon2x from 'leaflet/dist/images/marker-icon-2x.png';
import markerIcon from 'leaflet/dist/images/marker-icon.png';
import markerShadow from 'leaflet/dist/images/marker-shadow.png';

delete (L.Icon.Default.prototype as unknown as Record<string, unknown>)._getIconUrl;
L.Icon.Default.mergeOptions({
  iconRetinaUrl: markerIcon2x,
  iconUrl: markerIcon,
  shadowUrl: markerShadow,
});

const mqttIcon = new L.Icon({
  iconUrl: markerIcon,
  iconRetinaUrl: markerIcon2x,
  shadowUrl: markerShadow,
  iconSize: [25, 41],
  iconAnchor: [12, 41],
  popupAnchor: [1, -34],
  shadowSize: [41, 41],
  className: 'mqtt-marker',
});

function formatLastSeen(ts: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - ts;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

interface Props {
  nodes: DashboardNode[] | null;
}

export function NodeMap({ nodes }: Props) {
  const nodesWithPosition = useMemo(
    () => (nodes ?? []).filter((n) => n.latitude != null && n.longitude != null),
    [nodes],
  );

  if (nodesWithPosition.length === 0) {
    return (
      <div className="bg-slate-800 rounded-lg p-4">
        <h2 className="text-lg font-semibold mb-3">Node Map</h2>
        <p className="text-slate-400 text-sm">No nodes with position data available.</p>
      </div>
    );
  }

  const center: [number, number] = [
    nodesWithPosition.reduce((s, n) => s + n.latitude!, 0) / nodesWithPosition.length,
    nodesWithPosition.reduce((s, n) => s + n.longitude!, 0) / nodesWithPosition.length,
  ];

  return (
    <div className="bg-slate-800 rounded-lg p-4">
      <h2 className="text-lg font-semibold mb-3">Node Map</h2>
      <div className="rounded-lg overflow-hidden" style={{ height: '400px' }}>
        <MapContainer center={center} zoom={12} style={{ height: '100%', width: '100%' }}>
          <TileLayer
            attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
            url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
          />
          {nodesWithPosition.map((node) => (
            <Marker
              key={node.node_id}
              position={[node.latitude!, node.longitude!]}
              icon={node.via_mqtt ? mqttIcon : new L.Icon.Default()}
            >
              <Popup>
                <div className="text-sm">
                  <strong>{node.long_name || node.short_name || node.node_id}</strong>
                  <br />
                  {node.node_id}
                  <br />
                  {node.via_mqtt ? 'MQTT' : 'RF'} &middot; {formatLastSeen(node.last_seen)}
                </div>
              </Popup>
            </Marker>
          ))}
        </MapContainer>
      </div>
    </div>
  );
}

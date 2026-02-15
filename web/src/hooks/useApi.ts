import { useState, useEffect, useCallback } from 'react';
import type { MqttFilterValue } from '../types';

export function useApi<T>(path: string, mqtt: MqttFilterValue, extraParams?: Record<string, string>) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchData = useCallback(async () => {
    const params = new URLSearchParams({ mqtt, ...extraParams });
    const res = await fetch(`${path}?${params}`);
    if (res.ok) {
      setData(await res.json());
    }
    setLoading(false);
  }, [path, mqtt, extraParams]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  return { data, loading, refetch: fetchData };
}

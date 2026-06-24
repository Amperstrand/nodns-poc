import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import { discoverZones, type DiscoveredZone } from "@/lib/zones";

interface ZoneContextValue {
  zones: DiscoveredZone[];
  selectedZone: DiscoveredZone | null;
  selectZone: (zone: string) => void;
  loading: boolean;
  error: string | null;
}

const ZoneContext = createContext<ZoneContextValue | null>(null);

export function ZoneProvider({ children }: { children: ReactNode }) {
  const [zones, setZones] = useState<DiscoveredZone[]>([]);
  const [selectedZone, setSelectedZone] = useState<DiscoveredZone | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    async function load() {
      setLoading(true);
      setError(null);
      try {
        const discovered = await discoverZones();
        if (disposed) return;
        setZones(discovered);
        if (discovered.length > 0) {
          const verified = discovered.filter((z) => z.verified);
          setSelectedZone(verified[0] ?? discovered[0]);
        }
      } catch (e) {
        if (disposed) return;
        setError(e instanceof Error ? e.message : "Failed to discover zones");
      } finally {
        if (!disposed) setLoading(false);
      }
    }
    load();
    return () => {
      disposed = true;
    };
  }, []);

  const selectZone = useCallback(
    (zone: string) => {
      setSelectedZone((prev) => {
        const found = zones.find((z) => z.zone === zone);
        return found ?? prev;
      });
    },
    [zones],
  );

  return (
    <ZoneContext.Provider
      value={{ zones, selectedZone, selectZone, loading, error }}
    >
      {children}
    </ZoneContext.Provider>
  );
}

export function useZones(): ZoneContextValue {
  const ctx = useContext(ZoneContext);
  if (!ctx) {
    throw new Error("useZones must be used within ZoneProvider");
  }
  return ctx;
}

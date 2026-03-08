import { create } from 'zustand'

export interface FlightDetail {
  icao24: string
  callsign: string
  origin_country: string
  /** Live track waypoints — [[lon, lat, alt], ...] */
  track: [number, number, number][] | null
  /** Recent flights for this aircraft */
  flights: FlightRecord[] | null
  loading: boolean
}

export interface FlightRecord {
  icao24: string
  firstSeen: number
  lastSeen: number
  estDepartureAirport: string | null
  estArrivalAirport: string | null
  callsign: string | null
}

interface FlightDetailState {
  detail: FlightDetail | null
  select: (icao24: string, callsign: string, origin_country: string) => void
  setTrack: (track: [number, number, number][]) => void
  setFlights: (flights: FlightRecord[]) => void
  setLoading: (loading: boolean) => void
  close: () => void
}

export const useFlightDetailStore = create<FlightDetailState>((set) => ({
  detail: null,
  select: (icao24, callsign, origin_country) =>
    set({
      detail: { icao24, callsign, origin_country, track: null, flights: null, loading: true },
    }),
  setTrack: (track) =>
    set((s) => (s.detail ? { detail: { ...s.detail, track } } : {})),
  setFlights: (flights) =>
    set((s) => (s.detail ? { detail: { ...s.detail, flights } } : {})),
  setLoading: (loading) =>
    set((s) => (s.detail ? { detail: { ...s.detail, loading } } : {})),
  close: () => set({ detail: null }),
}))

import { create } from 'zustand'

export interface ShipDetail {
  mmsi: number
  ship_name: string
  ship_type: number | null
  course: number | null
  speed: number | null
  heading: number | null
  imo: number | null
  callsign: string | null
  destination: string | null
  eta: string | null
  draught: number | null
  length: number | null
  beam: number | null
  nav_status: number | null
}

interface ShipDetailState {
  detail: ShipDetail | null
  select: (props: ShipDetail) => void
  close: () => void
}

export const useShipDetailStore = create<ShipDetailState>((set) => ({
  detail: null,
  select: (props) => set({ detail: props }),
  close: () => set({ detail: null }),
}))

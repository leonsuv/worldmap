import { requestRedraw } from './scheduler'

export interface FlightTrack {
  icao24: string
  /** [lon, lat, altitude m] */
  path: [number, number, number][]
}

export interface ShipTrack {
  mmsi: number
  /** [lon, lat, unix seconds] */
  points: [number, number, number][]
}

let flight: FlightTrack | null = null
let ship: ShipTrack | null = null

export const getFlightTrack = () => flight
export const getShipTrack = () => ship

export function setFlightTrack(track: FlightTrack | null) {
  flight = track
  requestRedraw()
}

export function setShipTrack(track: ShipTrack | null) {
  ship = track
  requestRedraw()
}

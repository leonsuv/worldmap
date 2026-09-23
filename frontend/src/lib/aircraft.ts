import { rampColor } from './scale'

/** Altitude colour ramp: low = warm, cruise = cool (0 … 13,000 m). */
export const ALTITUDE_STOPS = ['#f7e27b', '#f5a54a', '#ea5f6b', '#b35fcf', '#6e7ff2', '#4cc6e8']
export const MAX_ALTITUDE = 13_000

export function altitudeColor(altitude: number | null, onGround: boolean, alpha = 235): [number, number, number, number] {
  if (onGround) return [150, 160, 166, 200]
  if (altitude == null) return [185, 192, 196, alpha]
  return rampColor(ALTITUDE_STOPS, altitude / MAX_ALTITUDE, alpha)
}

export const AIRCRAFT_CATEGORY: Record<number, string> = {
  2: 'Light aircraft', 3: 'Small aircraft', 4: 'Large aircraft', 5: 'High-vortex large aircraft', 6: 'Heavy aircraft',
  7: 'High-performance aircraft', 8: 'Rotorcraft', 9: 'Glider', 10: 'Balloon or airship', 11: 'Parachutist', 12: 'Ultralight',
  14: 'Drone', 15: 'Space vehicle', 16: 'Emergency vehicle', 17: 'Service vehicle',
}

export const EMERGENCY_SQUAWKS: Record<string, string> = { '7500': 'Unlawful interference', '7600': 'Radio failure', '7700': 'Emergency' }

/** AIS ship types (ITU-R M.1371) grouped into map categories. */

export type ShipCategory =
  | 'cargo' | 'tanker' | 'passenger' | 'highspeed' | 'fishing' | 'special' | 'pleasure' | 'government' | 'sar' | 'other' | 'unknown'

export const SHIP_CATEGORIES: { key: ShipCategory; label: string; short: string; color: string }[] = [
  { key: 'cargo', label: 'Cargo', short: 'Cargo', color: '#74c46a' },
  { key: 'tanker', label: 'Tanker', short: 'Tanker', color: '#ec6a6a' },
  { key: 'passenger', label: 'Passenger', short: 'Passenger', color: '#5ea8f2' },
  { key: 'highspeed', label: 'High-speed craft', short: 'High-speed', color: '#efd24a' },
  { key: 'fishing', label: 'Fishing', short: 'Fishing', color: '#f29a45' },
  { key: 'special', label: 'Tug, pilot & service', short: 'Service', color: '#3ec4c4' },
  { key: 'pleasure', label: 'Sailing & pleasure', short: 'Pleasure', color: '#c38ee6' },
  { key: 'government', label: 'Military & law enforcement', short: 'Government', color: '#9eabb4' },
  { key: 'sar', label: 'Search & rescue', short: 'SAR', color: '#ff78b0' },
  { key: 'other', label: 'Other', short: 'Other', color: '#c2a57c' },
  { key: 'unknown', label: 'Type not yet reported', short: 'Unknown', color: '#8b9ea8' },
]

export const CATEGORY_COLOR = Object.fromEntries(SHIP_CATEGORIES.map(c => [c.key, c.color])) as Record<ShipCategory, string>

export function shipCategory(type: number | null | undefined, sar = false): ShipCategory {
  if (sar) return 'sar'
  if (type == null || type <= 0) return 'unknown'
  if (type >= 70 && type <= 79) return 'cargo'
  if (type >= 80 && type <= 89) return 'tanker'
  if (type >= 60 && type <= 69) return 'passenger'
  if (type >= 40 && type <= 49) return 'highspeed'
  if (type === 30) return 'fishing'
  if (type === 36 || type === 37) return 'pleasure'
  if (type === 35 || type === 55) return 'government'
  if (type === 51) return 'sar'
  if ([31, 32, 33, 34, 50, 52, 53, 54, 58, 59].includes(type)) return 'special'
  return 'other'
}

const TYPE_LABELS: Record<number, string> = {
  30: 'Fishing', 31: 'Towing', 32: 'Towing (large)', 33: 'Dredging or underwater ops', 34: 'Diving ops', 35: 'Military ops',
  36: 'Sailing', 37: 'Pleasure craft', 50: 'Pilot vessel', 51: 'Search and rescue', 52: 'Tug', 53: 'Port tender',
  54: 'Anti-pollution', 55: 'Law enforcement', 58: 'Medical transport', 59: 'Non-combatant ship',
}
const DECADE_LABELS: Record<number, string> = { 20: 'Wing in ground', 40: 'High-speed craft', 60: 'Passenger', 70: 'Cargo', 80: 'Tanker', 90: 'Other type' }
const HAZARD: Record<number, string> = { 1: 'hazard A', 2: 'hazard B', 3: 'hazard C', 4: 'hazard D' }

export function shipTypeLabel(type: number | null | undefined): string {
  if (type == null) return 'Type not reported'
  if (TYPE_LABELS[type]) return TYPE_LABELS[type]
  const decade = Math.floor(type / 10) * 10
  const base = DECADE_LABELS[decade]
  if (!base) return `Type ${type}`
  const hazard = HAZARD[type - decade]
  return hazard ? `${base} (${hazard})` : base
}

export const NAV_STATUS: Record<number, string> = {
  0: 'Under way using engine', 1: 'At anchor', 2: 'Not under command', 3: 'Restricted manoeuvrability', 4: 'Constrained by draught',
  5: 'Moored', 6: 'Aground', 7: 'Engaged in fishing', 8: 'Under way sailing', 11: 'Towing astern', 12: 'Pushing ahead or towing alongside',
  14: 'AIS-SART / emergency',
}

export const ATON_TYPES: Record<number, string> = {
  0: 'Aid to navigation', 1: 'Reference point', 2: 'RACON', 3: 'Fixed offshore structure', 4: 'Emergency wreck marking',
  5: 'Light', 6: 'Sector light', 7: 'Leading light (front)', 8: 'Leading light (rear)', 9: 'North cardinal beacon', 10: 'East cardinal beacon',
  11: 'South cardinal beacon', 12: 'West cardinal beacon', 13: 'Port-hand beacon', 14: 'Starboard-hand beacon', 15: 'Preferred channel port beacon',
  16: 'Preferred channel starboard beacon', 17: 'Isolated danger beacon', 18: 'Safe water beacon', 19: 'Special mark beacon',
  20: 'North cardinal buoy', 21: 'East cardinal buoy', 22: 'South cardinal buoy', 23: 'West cardinal buoy', 24: 'Port-hand buoy',
  25: 'Starboard-hand buoy', 26: 'Preferred channel port buoy', 27: 'Preferred channel starboard buoy', 28: 'Isolated danger buoy',
  29: 'Safe water buoy', 30: 'Special mark buoy', 31: 'Light vessel, LANBY or rig',
}

/** True when a vessel is effectively stationary (drawn as a dot instead of an arrow). */
export function isStationary(speed: number | null | undefined, navStatus: number | null | undefined): boolean {
  if (navStatus === 1 || navStatus === 5 || navStatus === 6) return true
  return speed == null || speed < 0.5
}

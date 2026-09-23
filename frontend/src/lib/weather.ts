/** WMO weather interpretation codes used by Open-Meteo. */
const CODES: [number[], string][] = [
  [[0], 'Clear sky'],
  [[1], 'Mainly clear'],
  [[2], 'Partly cloudy'],
  [[3], 'Overcast'],
  [[45, 48], 'Fog'],
  [[51, 53, 55], 'Drizzle'],
  [[56, 57], 'Freezing drizzle'],
  [[61, 63, 65], 'Rain'],
  [[66, 67], 'Freezing rain'],
  [[71, 73, 75, 77], 'Snow'],
  [[80, 81, 82], 'Rain showers'],
  [[85, 86], 'Snow showers'],
  [[95], 'Thunderstorm'],
  [[96, 99], 'Thunderstorm with hail'],
]

export function weatherLabel(code: number | null | undefined): string {
  if (code == null) return 'Now'
  return CODES.find(([codes]) => codes.includes(code))?.[1] ?? 'Now'
}

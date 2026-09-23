import type { Theme } from '../store/ui'

interface Palette {
  halo: string
  label: string
  labelMuted: string
  airportLarge: string
  airportMedium: string
  port: string
  plantOperational: string
  plantConstruction: string
  plantSuspended: string
  atonPhysical: string
  atonVirtual: string
  atonOff: string
  building: string
  grid: string
}

export const PALETTES: Record<Theme, Palette> = {
  dark: {
    halo: '#0e171c',
    label: '#e3eaf5',
    labelMuted: '#aab8c0',
    airportLarge: '#a9bcff',
    airportMedium: '#8093d3',
    port: '#5fd0c4',
    plantOperational: '#f2d06b',
    plantConstruction: '#8ecae6',
    plantSuspended: '#9aa7b0',
    atonPhysical: '#f2d06b',
    atonVirtual: '#c38ee6',
    atonOff: '#ef6b6b',
    building: '#2e3f48',
    grid: '#d8c06a',
  },
  light: {
    halo: '#ffffff',
    label: '#1c2a55',
    labelMuted: '#4f5f68',
    airportLarge: '#3453c7',
    airportMedium: '#6179c8',
    port: '#0f7d73',
    plantOperational: '#c79a0e',
    plantConstruction: '#2b83b6',
    plantSuspended: '#7b8791',
    atonPhysical: '#b8900c',
    atonVirtual: '#8a4fc0',
    atonOff: '#cf3b3b',
    building: '#d9dcd7',
    grid: '#a88d22',
  },
}

export const HV_COLORS = { v110: '#6fa8dc', v220: '#5fb865', v300: '#ef5b5b', v500: '#c26de0', hvdc: '#ffd24d' }
export const PIPELINE_COLORS = { oil: '#d0784a', gas: '#e2b85c', mixed: '#b58a54', hydrogen: '#5fc3b0', other: '#9aa5ab' }

export type RGBA = [number, number, number, number]
export const ACCENT: Record<Theme, RGBA> = { dark: [198, 231, 156, 255], light: [79, 122, 31, 255] }
export const WIND_STOPS = ['#8ecae6', '#5ea8f2', '#efd24a', '#f29a45', '#ef6b6b', '#c23b8a']
export const WIND_MAX = 25
export const TRAFFIC_STOPS = ['#3cb44b', '#f6c142', '#f08a28', '#d9312d', '#7a1b16']

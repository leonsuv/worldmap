import { memo } from 'react'
import { useLayerStore } from '../store/layers'

const CREDITS: Record<string, string> = {
  flights: 'OpenSky Network',
  ships: 'AISstream.io',
  weather: 'Open-Meteo.com',
  traffic: 'TomTom',
  airports: 'OurAirports',
  seaports: 'OpenStreetMap / Overpass',
  reactors: 'IAEA PRIS',
  pipelines: 'OGIM',
  solar: 'OpenStreetMap',
  windTurbines: 'OpenStreetMap',
  buildings3d: 'OpenFreeMap / OSM',
}

function Attribution() {
  const store = useLayerStore()
  const active = Object.entries(CREDITS)
    .filter(([k]) => store[k as keyof typeof store])
    .map(([, v]) => v)

  // Always credit the basemap
  const sources = ['OpenFreeMap', ...new Set(active)]

  return (
    <div className="attribution">
      {sources.join(' · ')}
    </div>
  )
}

export default memo(Attribution)

import { memo, useState } from 'react'
import { mapInstance } from '../map/MapContainer'
import { ZoomIn, ZoomOut, Globe, Map, RotateCcw } from 'lucide-react'

const STYLES: Record<string, string> = {
  liberty: 'https://tiles.openfreemap.org/styles/liberty',
  dark: 'https://basemaps.cartocdn.com/gl/dark-matter-gl-style/style.json',
  satellite: 'https://api.maptiler.com/maps/hybrid/style.json?key=get_your_own_OpIi9ZULNHzrESv6T2vL',
}

function MapControls() {
  const [projection, setProjection] = useState<'globe' | 'mercator'>('mercator')
  const [theme, setTheme] = useState('liberty')

  const zoomIn = () => mapInstance?.zoomIn()
  const zoomOut = () => mapInstance?.zoomOut()
  const resetView = () => mapInstance?.flyTo({ center: [0, 20], zoom: 2 })

  const toggleProjection = () => {
    const next = projection === 'mercator' ? 'globe' : 'mercator'
    setProjection(next)
    mapInstance?.setProjection({ type: next })
  }

  const cycleTheme = () => {
    const keys = Object.keys(STYLES)
    const idx = (keys.indexOf(theme) + 1) % keys.length
    const next = keys[idx]
    setTheme(next)
    mapInstance?.setStyle(STYLES[next])
  }

  return (
    <div className="map-controls">
      <button title="Zoom in" onClick={zoomIn}><ZoomIn size={18} /></button>
      <button title="Zoom out" onClick={zoomOut}><ZoomOut size={18} /></button>
      <button title="Reset view" onClick={resetView}><RotateCcw size={18} /></button>
      <button title={projection === 'mercator' ? 'Globe view' : 'Flat view'} onClick={toggleProjection}>
        {projection === 'mercator' ? <Globe size={18} /> : <Map size={18} />}
      </button>
      <button title={`Theme: ${theme}`} onClick={cycleTheme} className="map-controls-theme">
        {theme === 'liberty' ? '☀️' : theme === 'dark' ? '🌙' : '🛰️'}
      </button>
    </div>
  )
}

export default memo(MapControls)

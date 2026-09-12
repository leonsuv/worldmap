import { memo, useState } from 'react'
import { mapInstance } from '../map/runtime'
import { ZoomIn, ZoomOut, Globe2, Map, Compass, Moon, Sun } from 'lucide-react'
import { MAP_STYLES } from '../map/styles'

function MapControls() {
  const [projection, setProjection] = useState<'globe' | 'mercator'>('mercator')
  const [theme, setTheme] = useState<'dark' | 'light'>('dark')
  const toggleProjection = () => {
    if (!mapInstance?.isStyleLoaded()) return
    const next = mapInstance.getProjection()?.type === 'globe' ? 'mercator' : 'globe'
    mapInstance.setProjection({ type: next })
    setProjection(next)
  }
  const changeTheme = (next: 'dark' | 'light') => {
    if (!mapInstance || next === theme) return
    const map = mapInstance
    const type = map.getProjection()?.type ?? 'mercator'
    setTheme(next)
    document.documentElement.dataset.basemap = next
    map.once('style.load', () => map.setProjection({ type }))
    map.setStyle(MAP_STYLES[next])
  }
  return <>
    <div className="map-controls" aria-label="Map navigation">
      <button title="Zoom in" aria-label="Zoom in" onClick={() => mapInstance?.zoomIn()}><ZoomIn size={18} /></button>
      <button title="Zoom out" aria-label="Zoom out" onClick={() => mapInstance?.zoomOut()}><ZoomOut size={18} /></button>
      <span className="control-divider" />
      <button title="Reset orientation" aria-label="Reset orientation" onClick={() => mapInstance?.easeTo({ bearing: 0, pitch: 0 })}><Compass size={19} /></button>
      <button title={projection === 'mercator' ? 'Globe view' : 'Flat view'} aria-label={projection === 'mercator' ? 'Globe view' : 'Flat view'} aria-pressed={projection === 'globe'} onClick={toggleProjection}>{projection === 'mercator' ? <Globe2 size={18} /> : <Map size={18} />}</button>
    </div>
    <div className="basemap-picker" aria-label="Basemap"><span>BASEMAP</span><button aria-pressed={theme === 'dark'} onClick={() => changeTheme('dark')}><Moon size={13} />Dark</button><button aria-pressed={theme === 'light'} onClick={() => changeTheme('light')}><Sun size={13} />Light</button></div>
  </>
}
export default memo(MapControls)

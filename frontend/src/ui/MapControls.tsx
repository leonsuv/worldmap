import { useEffect, useRef, useState } from 'react'
import { Box, Compass, Globe, History, Map as MapIcon, MapPinned, Minus, Moon, Plus, Sun } from 'lucide-react'
import { getMap } from '../map/runtime'
import { useUi } from '../store/ui'
import { useHistory } from '../store/history'
import { REGIONS } from '../catalog'

export default function MapControls() {
  const theme = useUi(s => s.theme)
  const setTheme = useUi(s => s.setTheme)
  const projection = useUi(s => s.projection)
  const setProjection = useUi(s => s.setProjection)
  const historyOn = useHistory(s => s.enabled)
  const [pitched, setPitched] = useState(false)
  const [regions, setRegions] = useState(false)
  const menu = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const map = getMap()
    if (!map) return
    const update = () => setPitched(map.getPitch() > 5)
    map.on('pitchend', update)
    return () => {
      map.off('pitchend', update)
    }
  })

  useEffect(() => {
    if (!regions) return
    const close = (e: PointerEvent) => !menu.current?.contains(e.target as Node) && setRegions(false)
    window.addEventListener('pointerdown', close)
    return () => window.removeEventListener('pointerdown', close)
  }, [regions])

  const map = () => getMap()
  return (
    <div className="map-controls">
      <div className="control-group surface" role="group" aria-label="Zoom and orientation">
        <button className="icon-btn" title="Zoom in" aria-label="Zoom in" onClick={() => map()?.zoomIn()}>
          <Plus size={18} />
        </button>
        <button className="icon-btn" title="Zoom out" aria-label="Zoom out" onClick={() => map()?.zoomOut()}>
          <Minus size={18} />
        </button>
        <hr />
        <button className="icon-btn" title="Reset north and tilt" aria-label="Reset north and tilt" onClick={() => map()?.easeTo({ bearing: 0, pitch: 0, duration: 600 })}>
          <Compass size={18} />
        </button>
        <button
          className="icon-btn"
          title={pitched ? 'Flat view' : 'Tilted 3D view'}
          aria-label="Toggle 3D tilt"
          aria-pressed={pitched}
          onClick={() => map()?.easeTo({ pitch: pitched ? 0 : 55, duration: 700 })}
        >
          <Box size={18} />
        </button>
        <button
          className="icon-btn"
          title={projection === 'globe' ? 'Flat map' : 'Globe'}
          aria-label="Toggle globe"
          aria-pressed={projection === 'globe'}
          onClick={() => setProjection(projection === 'globe' ? 'mercator' : 'globe')}
        >
          {projection === 'globe' ? <MapIcon size={18} /> : <Globe size={18} />}
        </button>
      </div>
      <div className="control-group surface" role="group" aria-label="View" ref={menu} style={{ position: 'relative' }}>
        <button className="icon-btn" title="Jump to a region" aria-label="Jump to a region" aria-expanded={regions} onClick={() => setRegions(r => !r)}>
          <MapPinned size={18} />
        </button>
        {regions && (
          <div className="region-menu surface" role="menu">
            {REGIONS.map(r => (
              <button
                key={r.name}
                role="menuitem"
                onClick={() => {
                  setRegions(false)
                  map()?.flyTo({ center: r.center, zoom: r.zoom, bearing: 0, pitch: 0, duration: 1400 })
                }}
              >
                {r.name}
              </button>
            ))}
          </div>
        )}
        <button
          className="icon-btn"
          title={theme === 'dark' ? 'Light map' : 'Dark map'}
          aria-label="Toggle light and dark map"
          onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
        >
          {theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}
        </button>
        <button
          className="icon-btn"
          title="Replay recorded vessel positions"
          aria-label="Historical replay"
          aria-pressed={historyOn}
          onClick={() => (historyOn ? useHistory.getState().close() : void useHistory.getState().open())}
        >
          <History size={18} />
        </button>
      </div>
    </div>
  )
}

import { Globe2, ArrowUpRight, Crosshair } from 'lucide-react'
import { useViewportStore } from '../store/viewport'
import { useLayerStore } from '../store/layers'
import { useDataStatus } from '../store/dataStatus'
import { mapInstance } from '../map/runtime'

export default function MapHeader() {
  const center = useViewportStore(s => s.center)
  const zoom = useViewportStore(s => s.zoom)
  const layers = useLayerStore()
  const sources = useDataStatus(s => s.sources)
  const active = Object.entries(layers).filter(([, v]) => v === true).map(([k]) => k)
  const errors = active.filter(k => sources[k]?.state === 'error').length
  const loading = active.some(k => sources[k]?.state === 'loading')
  const longitude = ((center[0] + 180) % 360 + 360) % 360 - 180
  return <>
    <header className="app-header">
      <div className="brand"><span className="brand-symbol"><Globe2 size={23} strokeWidth={1.5} /></span><div><h1>WORLDMAP<span className="brand-dot">.</span></h1><p>GLOBAL INFRASTRUCTURE ATLAS</p></div></div>
      <div className="header-context"><span className="header-divider" />Explore the connected world<ArrowUpRight size={14} /></div>
    </header>
    <div className={`map-caption ${active.length ? 'has-layers' : ''}`}><span className="eyebrow">A PLANET IN PERSPECTIVE</span><h2>Every connection.<br />One world.</h2><p>Explore the systems that move our planet.</p>
      <div className="region-shortcuts" aria-label="Jump to region">
        {([{ name: 'World', center: [0, 20], zoom: 2 }, { name: 'Europe', center: [12, 49], zoom: 4 }, { name: 'Asia Pacific', center: [115, 20], zoom: 3 }] as const).map(region => <button key={region.name} onClick={() => mapInstance?.flyTo({ center: [...region.center], zoom: region.zoom, duration: 1000 })}>{region.name}<ArrowUpRight size={12} /></button>)}
      </div>
    </div>
    <footer className="map-status"><span className={`status-dot ${errors ? 'is-error' : loading ? 'is-loading' : ''}`} /><span>{errors ? `${errors} source${errors > 1 ? 's' : ''} unavailable` : loading ? 'Updating map data' : active.length ? `${active.length} layers enabled` : 'Ready to explore'}</span><span className="status-coordinates"><Crosshair size={12} />{Math.abs(center[1]).toFixed(2)}° {center[1] < 0 ? 'S' : 'N'}<span> / </span>{Math.abs(longitude).toFixed(2)}° {longitude < 0 ? 'W' : 'E'}<span className="zoom-readout">Z {zoom.toFixed(1)}</span></span></footer>
  </>
}

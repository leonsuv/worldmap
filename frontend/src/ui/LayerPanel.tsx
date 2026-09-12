import { memo, useState } from 'react'
import { useLayerStore, type LayerState } from '../store/layers'
import { useDataStatus } from '../store/dataStatus'
import { useViewportStore } from '../store/viewport'
import { Plane, Ship, Wind, Atom, Factory, Cable, UtilityPole, Car, TowerControl, Anchor, Building2, Lightbulb, Layers3, ChevronDown, SlidersHorizontal, X } from 'lucide-react'
import type { ComponentType, CSSProperties } from 'react'

type LayerKey = keyof Omit<LayerState, 'toggle'>
interface LayerDef { key: LayerKey; label: string; icon: ComponentType<{ size?: number }>; group: string; source: string; color: string; minZoom?: number }
const LAYERS: LayerDef[] = [
  { key: 'flights', label: 'Flights', icon: Plane, group: 'Live activity', source: 'OpenSky Network', color: '#b7d977' },
  { key: 'ships', label: 'Vessels', icon: Ship, group: 'Live activity', source: 'AISstream · Live positions', color: '#6ed7cb' },
  { key: 'weather', label: 'Wind & weather', icon: Wind, group: 'Live activity', source: 'Open-Meteo', color: '#88bcf4' },
  { key: 'traffic', label: 'Road traffic', icon: Car, group: 'Live activity', source: 'TomTom · Street level', color: '#f3ab7b', minZoom: 10 },
  { key: 'airports', label: 'Airports', icon: TowerControl, group: 'Transport & places', source: 'OurAirports', color: '#91adf2' },
  { key: 'seaports', label: 'Seaports', icon: Anchor, group: 'Transport & places', source: 'OpenStreetMap', color: '#6ed7cb' },
  { key: 'aton', label: 'Navigation aids', icon: Lightbulb, group: 'Transport & places', source: 'AISstream · Buoys & beacons', color: '#e3cf8a' },
  { key: 'reactors', label: 'Nuclear reactors', icon: Atom, group: 'Energy networks', source: 'IAEA PRIS', color: '#e3cf8a' },
  { key: 'pipelines', label: 'Pipelines', icon: Factory, group: 'Energy networks', source: 'OGIM · Oil & gas', color: '#efa68a' },
  { key: 'powerGrid', label: 'Power grid', icon: Cable, group: 'Energy networks', source: 'Gridfinder · Estimated network', color: '#e3cf8a' },
  { key: 'hvLines', label: 'High-voltage lines', icon: UtilityPole, group: 'Energy networks', source: 'OpenStreetMap', color: '#b6a4ed', minZoom: 4 },
  { key: 'buildings3d', label: '3D buildings', icon: Building2, group: 'Built environment', source: 'OpenStreetMap · Street level', color: '#aabbd0', minZoom: 14 },
]
const GROUPS = [...new Set(LAYERS.map(l => l.group))]
const PRESETS: { name: string; keys: LayerKey[]; icon: typeof Plane }[] = [
  { name: 'Aviation', keys: ['flights', 'airports'], icon: Plane },
  { name: 'Maritime', keys: ['ships', 'seaports'], icon: Ship },
  { name: 'Energy', keys: ['reactors', 'pipelines', 'hvLines'], icon: Atom },
]

function LayerPanel() {
  const store = useLayerStore()
  const sources = useDataStatus(s => s.sources)
  const zoom = useViewportStore(s => s.zoom)
  const [collapsed, setCollapsed] = useState(false)
  const [filter, setFilter] = useState('')
  const [activeOnly, setActiveOnly] = useState(false)
  const count = LAYERS.filter(l => store[l.key]).length
  const applyPreset = (keys: LayerKey[]) => useLayerStore.setState(Object.fromEntries(LAYERS.map(l => [l.key, keys.includes(l.key)])))
  return <aside className={`layer-panel ${collapsed ? 'collapsed' : ''}`} aria-label="Map layers">
    <button className="layer-panel-heading" aria-expanded={!collapsed} onClick={() => setCollapsed(v => !v)}><span className="panel-heading-icon"><Layers3 size={18} /></span><span>Map layers<small>Build your perspective</small></span><span className="layer-count">{count.toString().padStart(2, '0')}</span><ChevronDown className="collapse-chevron" size={16} /></button>
    {!collapsed && <>
      <div className="layer-panel-body">
        <div className="preset-label">QUICK VIEWS</div>
        <div className="layer-presets">{PRESETS.map(({ name, keys, icon: Icon }) => <button key={name} onClick={() => applyPreset(keys)} aria-pressed={keys.every(k => store[k]) && count === keys.length}><Icon size={16} /><span>{name}</span></button>)}</div>
        <div className="layer-filter"><SlidersHorizontal size={14} /><input aria-label="Filter layers" placeholder="Find a layer…" value={filter} onChange={e => setFilter(e.target.value)} />{filter && <button aria-label="Clear layer filter" onClick={() => setFilter('')}><X size={13} /></button>}</div>
        <div className="layer-tabs"><button className={!activeOnly ? 'selected' : ''} onClick={() => setActiveOnly(false)}>All layers <span>{LAYERS.length}</span></button><button className={activeOnly ? 'selected' : ''} onClick={() => setActiveOnly(true)}>Enabled <span>{count}</span></button></div>
        <div className="layer-groups">{GROUPS.map(group => {
          const items = LAYERS.filter(l => l.group === group && (!activeOnly || store[l.key]) && `${l.label} ${l.source}`.toLowerCase().includes(filter.toLowerCase()))
          if (!items.length) return null
          return <section key={group} className="layer-group"><h3 className="layer-group-title">{group}<span>{items.length.toString().padStart(2, '0')}</span></h3>{items.map(l => {
            const Icon = l.icon
            const active = store[l.key]
            const status = sources[l.key]
            const hint = active && status?.state === 'error' ? status.message ?? 'Source unavailable · toggle to retry' : active && l.minZoom && zoom < l.minZoom ? `Zoom to level ${l.minZoom} to view` : active && status?.state === 'loading' ? 'Connecting to source…' : active && status?.count !== undefined ? `${status.count.toLocaleString()} ${status.count === 1 ? 'feature' : 'features'} · ${l.source.split(' · ')[0]}` : l.source
            return <label key={l.key} style={{ '--layer-color': l.color } as CSSProperties} className={`layer-row ${active ? 'active' : ''}`}><span className="layer-icon"><Icon size={17} /></span><span className="layer-label">{l.label}<span className={`layer-source ${active && status?.state === 'error' ? 'source-error' : ''}`}>{hint}</span></span><input aria-label={l.label} type="checkbox" className="layer-toggle" checked={active} onChange={() => store.toggle(l.key)} /></label>
          })}</section>
        })}{!LAYERS.some(l => (!activeOnly || store[l.key]) && `${l.label} ${l.source}`.toLowerCase().includes(filter.toLowerCase())) && <div className="layer-empty">{activeOnly && !count ? 'Enable a layer to start exploring.' : 'No matching layers.'}</div>}</div>
      </div>
      <div className="layer-footer"><span><span className="status-dot" />{count} of {LAYERS.length} enabled</span><button disabled={!count} onClick={() => applyPreset([])}>Clear all</button></div>
    </>}
  </aside>
}
export default memo(LayerPanel)

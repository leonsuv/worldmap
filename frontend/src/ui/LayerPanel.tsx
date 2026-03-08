import { memo } from 'react'
import { useLayerStore, type LayerState } from '../store/layers'
import {
  Plane, Ship, Wind, Atom, Factory, Cable, UtilityPole,
  Car, TowerControl, Anchor, Building2, Lightbulb,
} from 'lucide-react'
import type { ComponentType } from 'react'

type LayerKey = keyof Omit<LayerState, 'toggle'>

interface LayerDef {
  key: LayerKey
  label: string
  icon: ComponentType<{ size?: number }>
  group: string
  source: string
}

const LAYERS: LayerDef[] = [
  { key: 'flights',      label: 'Flights',         icon: Plane,        group: 'Live Data',       source: 'OpenSky Network' },
  { key: 'ships',        label: 'Ships (AIS)',      icon: Ship,         group: 'Live Data',       source: 'AISstream' },
  { key: 'aton',         label: 'Nav Aids (AtoN)',  icon: Lightbulb,    group: 'Live Data',       source: 'AISstream' },
  { key: 'weather',      label: 'Wind / Weather',   icon: Wind,         group: 'Live Data',       source: 'Open-Meteo' },
  { key: 'traffic',      label: 'Traffic',          icon: Car,          group: 'Live Data',       source: 'TomTom' },
  { key: 'airports',     label: 'Airports',         icon: TowerControl, group: 'Infrastructure',  source: 'OurAirports' },
  { key: 'seaports',     label: 'Seaports',         icon: Anchor,       group: 'Infrastructure',  source: 'OpenStreetMap' },
  { key: 'reactors',     label: 'Nuclear Reactors', icon: Atom,         group: 'Infrastructure',  source: 'IAEA PRIS' },
  { key: 'pipelines',    label: 'Pipelines',        icon: Factory,      group: 'Infrastructure',  source: 'OGIM' },
  { key: 'powerGrid',    label: 'Power Grid',       icon: Cable,        group: 'Energy',          source: 'Gridfinder' },
  { key: 'hvLines',      label: 'HV Transmission',  icon: UtilityPole,  group: 'Energy',          source: 'OSM' },
  { key: 'buildings3d',  label: '3D Buildings',      icon: Building2,    group: 'Environment',     source: 'OpenFreeMap' },
]

const GROUPS = ['Live Data', 'Infrastructure', 'Energy', 'Environment']

function LayerPanel() {
  const store = useLayerStore()

  return (
    <div className="layer-panel">
      <h2 className="layer-panel-title">Layers</h2>
      {GROUPS.map((group) => {
        const items = LAYERS.filter((l) => l.group === group)
        if (!items.length) return null
        return (
          <div key={group} className="layer-group">
            <h3 className="layer-group-title">{group}</h3>
            {items.map((l) => {
              const Icon = l.icon
              const active = store[l.key]
              return (
                <label key={l.key} className={`layer-row ${active ? 'active' : ''}`}>
                  <Icon size={16} />
                  <span className="layer-label">{l.label}</span>
                  <span className="layer-source">{l.source}</span>
                  <input
                    type="checkbox"
                    className="layer-toggle"
                    checked={active}
                    onChange={() => store.toggle(l.key)}
                  />
                </label>
              )
            })}
          </div>
        )
      })}
    </div>
  )
}

export default memo(LayerPanel)

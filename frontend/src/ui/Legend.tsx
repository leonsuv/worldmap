import { ChevronDown } from 'lucide-react'
import { useLayers } from '../store/layers'
import { useStatus, availability } from '../store/status'
import { useUi } from '../store/ui'
import { SHIP_CATEGORIES } from '../lib/ais'
import { ALTITUDE_STOPS } from '../lib/aircraft'
import { HV_COLORS, PALETTES, PIPELINE_COLORS, TRAFFIC_STOPS, WIND_STOPS } from '../map/colors'
import type { LayerKey } from '../catalog'

function Item({ color, label, line, dashed }: { color: string; label: string; line?: boolean; dashed?: boolean }) {
  return (
    <div className="legend-item">
      <span className={line ? `swatch-line ${dashed ? 'dashed' : ''}` : 'dot'} style={{ '--dot': color } as React.CSSProperties} />
      <span>{label}</span>
    </div>
  )
}

function Ramp({ stops, from, to }: { stops: string[]; from: string; to: string }) {
  return (
    <>
      <div className="ramp" style={{ background: `linear-gradient(90deg, ${stops.join(', ')})` }} />
      <div className="ramp-labels">
        <span>{from}</span>
        <span>{to}</span>
      </div>
    </>
  )
}

export default function Legend() {
  const enabled = useLayers(s => s.enabled)
  const caps = useStatus(s => s.caps)
  const theme = useUi(s => s.theme)
  const open = useUi(s => s.legendOpen)
  const setOpen = useUi(s => s.setLegendOpen)
  const globe = useUi(s => s.projection === 'globe')
  const on = (k: LayerKey) => enabled[k] && availability(k, caps, false).ok
  const p = PALETTES[theme]

  const blocks = [
    on('flights') && (
      <div className="legend-block" key="flights">
        <h4>Aircraft altitude</h4>
        <Ramp stops={ALTITUDE_STOPS} from="Ground" to="13,000 m" />
      </div>
    ),
    on('ships') && (
      <div className="legend-block" key="ships">
        <h4>Vessels</h4>
        <div className="legend-items">
          {SHIP_CATEGORIES.filter(c => c.key !== 'unknown' && c.key !== 'other').map(c => (
            <Item key={c.key} color={c.color} label={c.short} />
          ))}
        </div>
        <p className="muted" style={{ marginTop: 6 }}>Arrows move, dots are moored or anchored.</p>
      </div>
    ),
    on('weather') && (
      <div className="legend-block" key="weather">
        <h4>Wind speed (arrows point downwind)</h4>
        <Ramp stops={WIND_STOPS} from="0 m/s" to="25+ m/s" />
      </div>
    ),
    on('hvLines') && (
      <div className="legend-block" key="hv">
        <h4>Transmission lines</h4>
        <div className="legend-items">
          <Item line color={HV_COLORS.v110} label="110–219 kV" />
          <Item line color={HV_COLORS.v220} label="220–299 kV" />
          <Item line color={HV_COLORS.v300} label="300–499 kV" />
          <Item line color={HV_COLORS.v500} label="500 kV +" />
          <Item line color={HV_COLORS.hvdc} label="HVDC" />
          <Item line color={p.labelMuted} label="Cable: fine dash" dashed />
        </div>
      </div>
    ),
    on('pipelines') && (
      <div className="legend-block" key="pipes">
        <h4>Pipelines</h4>
        <div className="legend-items">
          <Item line dashed color={PIPELINE_COLORS.oil} label="Oil & products" />
          <Item line dashed color={PIPELINE_COLORS.gas} label="Gas" />
          <Item line dashed color={PIPELINE_COLORS.mixed} label="Oil and gas" />
          <Item line dashed color={PIPELINE_COLORS.hydrogen} label="Hydrogen" />
        </div>
      </div>
    ),
    on('reactors') && (
      <div className="legend-block" key="plants">
        <h4>Nuclear plants (size = capacity)</h4>
        <div className="legend-items">
          <Item color={p.plantOperational} label="Operating" />
          <Item color={p.plantConstruction} label="Under construction" />
          <Item color={p.plantSuspended} label="Suspended" />
        </div>
      </div>
    ),
    (on('airports') || on('seaports') || on('powerGrid')) && (
      <div className="legend-block" key="points">
        <div className="legend-items">
          {on('airports') && <Item color={p.airportLarge} label="Major airport" />}
          {on('airports') && <Item color={p.airportMedium} label="Airport" />}
          {on('seaports') && <Item color={p.port} label="Seaport" />}
          {on('powerGrid') && <Item line color={p.grid} label="Estimated grid" />}
        </div>
      </div>
    ),
    on('traffic') && (
      <div className="legend-block" key="traffic">
        <h4>Road traffic</h4>
        <Ramp stops={TRAFFIC_STOPS} from="Free flow" to="Standstill" />
      </div>
    ),
  ].filter(Boolean)

  if (!blocks.length) return null
  return (
    <section className={`legend surface ${open ? '' : 'closed'}`} aria-label="Legend">
      <button className="legend-head" style={{ width: '100%' }} aria-expanded={open} onClick={() => setOpen(!open)}>
        <h3>Legend</h3>
        <ChevronDown size={15} style={{ transform: open ? 'rotate(180deg)' : undefined, color: 'var(--text-3)' }} />
      </button>
      {open && blocks}
      {open && globe && (on('flights') || on('ships') || on('weather')) && (
        <p className="muted" style={{ marginTop: 8 }}>Globe view shows aircraft, vessels and wind as dots.</p>
      )}
    </section>
  )
}

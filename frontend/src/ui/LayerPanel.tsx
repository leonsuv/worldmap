import { useState } from 'react'
import { ChevronDown, Layers, RefreshCw } from 'lucide-react'
import { GROUPS, LAYERS, PRESETS, type LayerDef, type LayerKey } from '../catalog'
import { useLayers } from '../store/layers'
import { availability, useFeeds, useStatus, type Availability, type FeedStatus } from '../store/status'
import { useViewport } from '../store/viewport'
import { useUi } from '../store/ui'
import { useHistory } from '../store/history'
import { fmtCount, timeAgo } from '../lib/format'

function describe(def: LayerDef, on: boolean, avail: Availability, feed: FeedStatus | undefined, zoom: number, historyOn: boolean): { text: string; tone?: 'warn' | 'error' } {
  if (!avail.ok) return { text: avail.reason ?? 'Unavailable', tone: 'warn' }
  if (!on) return { text: def.source }
  if (def.key === 'ships' && historyOn) return { text: 'Paused during historical replay' }
  if (def.minZoom && zoom < def.minZoom) return { text: `Zoom in to level ${def.minZoom} to see it`, tone: 'warn' }
  if (!feed) return { text: def.source }
  if (feed.state === 'loading') return { text: 'Loading…' }
  if (feed.state === 'error') return { text: feed.message ?? 'Source unavailable', tone: 'error' }
  if (feed.message) return { text: feed.message, tone: 'warn' }
  if (feed.count !== undefined) {
    const when = feed.updatedAt && ['flights', 'ships', 'weather'].includes(def.key) ? ` · ${timeAgo(feed.updatedAt)}` : ''
    return { text: `${fmtCount(feed.count)} ${feed.count === 1 ? 'item' : 'items'}${when}` }
  }
  return { text: def.source }
}

function LayerRow({ def }: { def: LayerDef }) {
  const on = useLayers(s => s.enabled[def.key])
  const toggle = useLayers(s => s.toggle)
  const caps = useStatus(s => s.caps)
  const error = useStatus(s => s.error)
  const feed = useFeeds(s => s.feeds[def.key])
  const zoom = useViewport(s => s.zoom)
  const historyOn = useHistory(s => s.enabled)
  const [showFix, setShowFix] = useState(false)
  const avail = availability(def.key, caps, error)
  const active = on && avail.ok
  const meta = describe(def, on, avail, feed, zoom, historyOn)
  const Icon = def.icon
  const id = `layer-${def.key}`
  return (
    <>
      <label className={`layer-row ${active ? 'on' : ''} ${avail.ok ? '' : 'unavailable'}`} style={{ '--layer-color': def.color } as React.CSSProperties} htmlFor={id}>
        <span className="layer-icon">
          <Icon size={17} />
        </span>
        <span style={{ minWidth: 0 }}>
          <span className="layer-name">{def.label}</span>
          <span className={`layer-meta ${meta.tone ?? ''}`}>{meta.text}</span>
        </span>
        {avail.ok ? (
          <input id={id} type="checkbox" className="switch" checked={active} onChange={() => toggle(def.key)} aria-label={def.label} />
        ) : (
          <button className="btn small ghost" onClick={e => { e.preventDefault(); setShowFix(v => !v) }} aria-expanded={showFix} disabled={!avail.fix}>
            Set up
          </button>
        )}
      </label>
      {!avail.ok && showFix && avail.fix && (
        <div className="layer-fix">
          {avail.fix.startsWith('Run: ') ? (
            <>
              Run this in the project folder, then click <b>Recheck</b>:<code>{avail.fix.slice(5)}</code>
            </>
          ) : (
            avail.fix
          )}
        </div>
      )}
    </>
  )
}

export default function LayerPanel() {
  const open = useUi(s => s.layersOpen)
  const setOpen = useUi(s => s.setLayersOpen)
  const enabled = useLayers(s => s.enabled)
  const only = useLayers(s => s.only)
  const caps = useStatus(s => s.caps)
  const statusError = useStatus(s => s.error)
  const checking = useStatus(s => s.checking)
  const refresh = useStatus(s => s.refresh)
  const count = LAYERS.filter(l => enabled[l.key] && availability(l.key, caps, statusError).ok).length

  const presetKeys = (keys: LayerKey[]) => keys.filter(k => availability(k, caps, statusError).ok)
  const presetActive = (keys: LayerKey[]) => {
    const usable = presetKeys(keys)
    return usable.length > 0 && LAYERS.every(l => enabled[l.key] === usable.includes(l.key) || !availability(l.key, caps, statusError).ok)
  }

  return (
    <aside className={`layer-panel surface ${open ? '' : 'collapsed'}`} aria-label="Map layers">
      <button className="panel-head" style={{ width: '100%', textAlign: 'left' }} aria-expanded={open} onClick={() => setOpen(!open)}>
        <Layers size={18} color="var(--accent)" />
        <h2>Layers</h2>
        <span className="count-pill">{count}</span>
        <ChevronDown size={16} style={{ transform: open ? undefined : 'rotate(-90deg)', transition: 'transform .15s', color: 'var(--text-3)' }} />
      </button>
      {open && (
        <>
          <div className="panel-body">
            <div className="section-label">Quick views</div>
            <div className="presets">
              {PRESETS.map(p => (
                <button key={p.name} className="preset" aria-pressed={presetActive(p.keys)} disabled={!presetKeys(p.keys).length} onClick={() => only(presetKeys(p.keys))}>
                  {p.name}
                </button>
              ))}
            </div>
            {statusError && !caps && (
              <p className="notice-inline" role="status">
                The WorldMap server is not reachable. Start it with <b>python scripts/dev.py</b>.
              </p>
            )}
            {GROUPS.map(group => (
              <section key={group} aria-label={group}>
                <div className="section-label">{group}</div>
                {LAYERS.filter(l => l.group === group).map(def => (
                  <LayerRow key={def.key} def={def} />
                ))}
              </section>
            ))}
          </div>
          <footer className="panel-foot">
            <button className="btn small ghost" onClick={() => only([])} disabled={!count}>
              Clear all
            </button>
            <button className="btn small ghost" onClick={() => void refresh()} disabled={checking} title="Check the server for new keys, tiles and datasets">
              <RefreshCw size={13} className={checking ? 'spin' : undefined} /> {checking ? 'Checking…' : 'Recheck'}
            </button>
          </footer>
        </>
      )}
    </aside>
  )
}

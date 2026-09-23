import { useLayers } from '../store/layers'
import { useFeeds, useStatus, availability } from '../store/status'
import { useViewport } from '../store/viewport'
import { LAYERS } from '../catalog'
import { fmtLat, fmtLon } from '../lib/format'

export default function StatusBar() {
  const enabled = useLayers(s => s.enabled)
  const caps = useStatus(s => s.caps)
  const serverDown = useStatus(s => s.error && !s.caps)
  const feeds = useFeeds(s => s.feeds)
  const zoom = useViewport(s => s.zoom)
  const center = useViewport(s => s.center)
  const pointer = useViewport(s => s.pointer)

  const active = LAYERS.filter(l => enabled[l.key] && availability(l.key, caps, false).ok)
  const errors = active.filter(l => feeds[l.key]?.state === 'error')
  const loading = active.some(l => feeds[l.key]?.state === 'loading')
  const [lon, lat] = pointer ?? center
  const sources = [...new Set(active.flatMap(l => l.attribution.split(' / ')))]

  let state: 'ok' | 'loading' | 'error' = 'ok'
  let text = active.length ? `${active.length} layer${active.length === 1 ? '' : 's'} on` : 'Ready — pick layers to explore'
  if (serverDown) {
    state = 'error'
    text = 'Server offline'
  } else if (errors.length) {
    state = 'error'
    text = `${errors.map(e => e.label).join(', ')} unavailable`
  } else if (loading) {
    state = 'loading'
    text = 'Updating map data…'
  }

  return (
    <footer className="statusbar">
      <span className={`status-indicator ${state}`} role="status">
        <span className="dot" />
        {text}
      </span>
      {sources.length > 0 && <span className="status-sources">Data: {sources.join(' · ')}</span>}
      <span className="status-coords" aria-label={pointer ? 'Pointer position' : 'Map centre'}>
        <span>
          {fmtLat(lat)} {fmtLon(lon)}
        </span>
        <span>z {zoom.toFixed(1)}</span>
      </span>
    </footer>
  )
}

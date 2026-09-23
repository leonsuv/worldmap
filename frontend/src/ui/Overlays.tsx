import { AlertCircle, Info, Pause, Play, SkipBack, SkipForward, X } from 'lucide-react'
import { useHover } from '../map/interaction'
import { useUi } from '../store/ui'
import { useNotices } from '../store/notice'
import { useHistory } from '../store/history'
import { fmtDateTime, fmtCount } from '../lib/format'

export function Tooltip() {
  const hover = useHover(s => s.hover)
  if (!hover) return null
  return (
    <div className="tooltip" style={{ left: hover.x, top: hover.y }}>
      <strong>{hover.title}</strong>
      {hover.subtitle && <small>{hover.subtitle}</small>}
    </div>
  )
}

export function PickBanner() {
  const pick = useUi(s => s.pick)
  const cancel = useUi(s => s.cancelPick)
  if (!pick) return null
  return (
    <div className="pick-banner surface" role="status">
      {pick.label}
      <button className="btn small" onClick={cancel}>
        Cancel
      </button>
    </div>
  )
}

export function Notices() {
  const notices = useNotices(s => s.notices)
  const dismiss = useNotices(s => s.dismiss)
  if (!notices.length) return null
  return (
    <div className="notices" aria-live="polite">
      {notices.map(n => (
        <div key={n.id} className={`notice ${n.tone}`} role={n.tone === 'error' ? 'alert' : 'status'}>
          {n.tone === 'error' ? <AlertCircle size={16} className="lead" /> : <Info size={16} className="lead" />}
          <p>{n.message}</p>
          <button className="icon-btn" aria-label="Dismiss" onClick={() => dismiss(n.id)}>
            <X size={15} />
          </button>
        </div>
      ))}
    </div>
  )
}

export function Timeline() {
  const { enabled, timestamps, index, loading, playing, error, positions } = useHistory()
  const { seek, setPlaying, close } = useHistory.getState()
  if (!enabled) return null
  const current = timestamps[index]
  const empty = !loading && !timestamps.length
  return (
    <section className="timeline surface" aria-label="Historical replay">
      <button className="icon-btn" aria-label={playing ? 'Pause' : 'Play'} disabled={timestamps.length < 2} onClick={() => setPlaying(!playing)}>
        {playing ? <Pause size={17} /> : <Play size={17} />}
      </button>
      <div className="timeline-label">
        <strong>{error ?? (empty ? 'No recordings yet' : current ? fmtDateTime(current, { weekday: 'short' }) : 'Loading…')}</strong>
        <small>
          {empty
            ? 'Vessel snapshots are saved every 5 minutes while AIS is connected.'
            : loading
              ? 'Loading snapshot…'
              : `${fmtCount(positions.length)} vessels · ${index + 1} of ${timestamps.length}`}
        </small>
      </div>
      <div className="row">
        <button className="icon-btn" aria-label="Previous snapshot" disabled={index <= 0} onClick={() => void seek(index - 1)}>
          <SkipBack size={15} />
        </button>
        <input
          type="range"
          aria-label="Snapshot time"
          min={0}
          max={Math.max(0, timestamps.length - 1)}
          value={Math.max(0, index)}
          disabled={timestamps.length < 2}
          onChange={e => void seek(Number(e.target.value))}
        />
        <button className="icon-btn" aria-label="Next snapshot" disabled={index >= timestamps.length - 1} onClick={() => void seek(index + 1)}>
          <SkipForward size={15} />
        </button>
      </div>
      <button className="icon-btn timeline-close" aria-label="Close replay" onClick={close}>
        <X size={16} />
      </button>
    </section>
  )
}

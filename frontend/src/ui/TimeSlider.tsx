import { memo, useEffect } from 'react'
import { useHistoryStore } from '../store/history'
import { History, X } from 'lucide-react'

function TimeSlider() {
  const { enabled, timestamps, currentTs, loading, toggle, fetchTimestamps, seek } = useHistoryStore()

  useEffect(() => {
    if (enabled && timestamps.length === 0) fetchTimestamps()
  }, [enabled, timestamps.length, fetchTimestamps])

  if (!enabled) {
    return (
      <button className="ts-toggle" onClick={toggle} title="Historical Replay">
        <History size={16} />
      </button>
    )
  }

  const min = timestamps.length > 0 ? timestamps[0] : 0
  const max = timestamps.length > 0 ? timestamps[timestamps.length - 1] : 0
  const value = currentTs ?? max

  const fmt = (ts: number) => {
    if (!ts) return '—'
    const d = new Date(ts * 1000)
    return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
  }

  return (
    <div className="ts-bar">
      <button className="ts-close" onClick={toggle}><X size={14} /></button>
      <History size={14} />
      <span className="ts-label">{loading ? 'Loading…' : fmt(value)}</span>
      <input
        type="range"
        className="ts-slider"
        min={min}
        max={max}
        step={300}
        value={value}
        onChange={e => seek(parseInt(e.target.value))}
      />
      <span className="ts-range">{fmt(min)} — {fmt(max)}</span>
    </div>
  )
}

export default memo(TimeSlider)

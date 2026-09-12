import { memo } from 'react'
import { useHistoryStore } from '../store/history'
import { History, X } from 'lucide-react'
function TimeSlider() {
  const { enabled, timestamps, currentTs, loading, error, toggle, seek } = useHistoryStore()
  if (!enabled) return <button className="ts-toggle" onClick={toggle} title="Historical replay" aria-label="Historical replay"><History size={17} /></button>
  const fmt = (ts: number) => new Date(ts * 1000).toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
  return <div className="ts-bar">
    <button className="ts-close" aria-label="Close historical replay" onClick={toggle}><X size={14} /></button><History size={14} />
    <span className="ts-label" role="status">{error ?? (loading ? 'Loading snapshot…' : currentTs ? fmt(currentTs) : 'No recorded snapshots')}</span>
    {timestamps.length > 0 && <><input type="range" aria-label="Historical snapshot" className="ts-slider" min={0} max={timestamps.length - 1} step={1} value={Math.max(0, timestamps.indexOf(currentTs ?? 0))} onChange={e => void seek(timestamps[Number(e.target.value)])} /><span className="ts-range">{fmt(timestamps[0])} — {fmt(timestamps[timestamps.length - 1])} · {timestamps.length} snapshots</span></>}
  </div>
}
export default memo(TimeSlider)

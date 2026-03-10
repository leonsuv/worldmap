import { memo } from 'react'
import { useWatchlistStore } from '../store/watchlist'
import { useEventStore } from '../store/events'
import { List, AlertTriangle } from 'lucide-react'

function BusinessToolbar() {
  const toggleWatchlist = useWatchlistStore(s => s.toggle)
  const toggleEvents = useEventStore(s => s.toggle)

  return (
    <div className="biz-toolbar">
      <button className="biz-btn" onClick={toggleWatchlist} title="Watchlist">
        <List size={15} /> Watchlist
      </button>
      <button className="biz-btn" onClick={toggleEvents} title="Events">
        <AlertTriangle size={15} /> Events
      </button>
    </div>
  )
}

export default memo(BusinessToolbar)

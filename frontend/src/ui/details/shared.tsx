import { useState } from 'react'
import { ListPlus, LocateFixed } from 'lucide-react'
import { useWatchlist, type WatchType, type WatchItem } from '../../store/watchlist'
import { notify } from '../../store/notice'
import { flyTo } from '../../map/runtime'

export function WatchButton({ wtype, name, params }: { wtype: WatchType; name: string; params: WatchItem['params'] }) {
  const items = useWatchlist(s => s.items)
  const add = useWatchlist(s => s.add)
  const [busy, setBusy] = useState(false)
  const already = items.some(i => i.wtype === wtype && (params.mmsi ? i.params.mmsi === params.mmsi : i.name === name && i.params.lat === params.lat))
  return (
    <button
      className="btn"
      disabled={busy || already}
      onClick={async () => {
        setBusy(true)
        if (await add(wtype, name, params)) notify(`${name} is on your watchlist`, 'info')
        setBusy(false)
      }}
    >
      <ListPlus size={15} /> {already ? 'On watchlist' : 'Watch'}
    </button>
  )
}

export function ZoomButton({ lon, lat, zoom }: { lon: number; lat: number; zoom?: number }) {
  return (
    <button className="btn" onClick={() => flyTo(lon, lat, zoom)}>
      <LocateFixed size={15} /> Zoom to
    </button>
  )
}

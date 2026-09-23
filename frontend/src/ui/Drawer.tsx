import { useEffect } from 'react'
import { useUi } from '../store/ui'
import { useSelection } from '../store/selection'
import FlightDetails from './details/FlightDetails'
import ShipDetails from './details/ShipDetails'
import PlaceDetails from './details/PlaceDetails'
import WatchlistPanel from './WatchlistPanel'
import EventsPanel from './EventsPanel'
import AlertsPanel from './AlertsPanel'

export default function Drawer() {
  const panel = useUi(s => s.panel)
  const selected = useSelection(s => s.selected)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      const target = e.target as HTMLElement
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return
      const ui = useUi.getState()
      if (ui.pick) return ui.cancelPick()
      if (ui.panel === 'details') useSelection.getState().clear()
      else if (ui.panel) ui.closePanel()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  switch (panel) {
    case 'watchlist':
      return <WatchlistPanel />
    case 'events':
      return <EventsPanel />
    case 'alerts':
      return <AlertsPanel />
    case 'details':
      if (!selected) return null
      if (selected.kind === 'flight') return <FlightDetails key={selected.id} id={selected.id} />
      if (selected.kind === 'ship') return <ShipDetails key={selected.id} mmsi={selected.id} />
      return <PlaceDetails key={`${selected.kind}-${selected.id}`} sel={selected} />
    default:
      return null
  }
}

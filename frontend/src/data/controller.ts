import { useEffect, useRef } from 'react'
import { useLayers } from '../store/layers'
import { useStatus, availability } from '../store/status'
import { useViewport } from '../store/viewport'
import { useAlerts } from '../store/alerts'
import { useEvents } from '../store/events'
import { startFlights, stopFlights } from './flights'
import { startShips, stopShips } from './ships'
import { requestWeather, stopWeather } from './weather'
import { invalidateCollection, loadCollection } from './staticData'

/** Start and stop live feeds as layers are toggled and capabilities change. */
export function useDataFeeds() {
  const enabled = useLayers(s => s.enabled)
  const caps = useStatus(s => s.caps)
  const bbox = useViewport(s => s.bbox)
  const shipsReady = !!caps?.ships_configured

  // Server capabilities: poll so newly built tiles or imported data appear by themselves.
  useEffect(() => {
    const { refresh } = useStatus.getState()
    void refresh()
    const id = setInterval(() => void refresh(), 60_000)
    return () => clearInterval(id)
  }, [])

  // Alerts badge and events (drawn on the map) stay current.
  useEffect(() => {
    const { fetchCount } = useAlerts.getState()
    void fetchCount()
    void useEvents.getState().fetch()
    const id = setInterval(() => !document.hidden && void fetchCount(), 30_000)
    return () => clearInterval(id)
  }, [])

  useEffect(() => {
    if (enabled.flights) startFlights()
    else stopFlights()
  }, [enabled.flights])

  useEffect(() => {
    if (enabled.ships && shipsReady) startShips()
    else stopShips()
  }, [enabled.ships, shipsReady])

  useEffect(() => () => {
    stopFlights()
    stopShips()
    stopWeather()
  }, [])

  useEffect(() => {
    if (enabled.weather) requestWeather(bbox)
    else stopWeather()
  }, [enabled.weather, bbox])

  // Imported datasets: load on first use, reload after a re-import.
  const counts = useRef('')
  useEffect(() => {
    const signature = caps ? JSON.stringify(caps.datasets) : ''
    if (counts.current && signature !== counts.current) {
      for (const key of ['airports', 'seaports', 'reactors'] as const) invalidateCollection(key)
    }
    counts.current = signature
    for (const key of ['airports', 'seaports', 'reactors'] as const) {
      if (enabled[key] && availability(key, caps, false).ok) void loadCollection(key)
    }
  }, [enabled, caps])

  useEffect(() => {
    if (!enabled.aton || !shipsReady) return
    void loadCollection('aton', true)
    const id = setInterval(() => !document.hidden && void loadCollection('aton', true), 60_000)
    return () => clearInterval(id)
  }, [enabled.aton, shipsReady])
}

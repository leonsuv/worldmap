import { useEffect, useRef } from 'react'
import maplibregl from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { MapboxOverlay } from '@deck.gl/mapbox'
import { useViewportStore } from '../store/viewport'

let debounceTimer: ReturnType<typeof setTimeout> | undefined

export let mapInstance: maplibregl.Map | null = null
export let deckOverlay: MapboxOverlay | null = null

export default function MapContainer() {
  const containerRef = useRef<HTMLDivElement>(null)
  const setViewport = useViewportStore((s) => s.setViewport)

  useEffect(() => {
    if (!containerRef.current) return

    const map = new maplibregl.Map({
      container: containerRef.current,
      style: 'https://tiles.openfreemap.org/styles/liberty',
      center: [0, 20],
      zoom: 2,
      hash: true,
    })

    mapInstance = map

    const overlay = new MapboxOverlay({ interleaved: true, layers: [] })
    map.addControl(overlay as unknown as maplibregl.IControl)
    deckOverlay = overlay

    map.addControl(new maplibregl.NavigationControl(), 'top-right')

    const updateViewport = () => {
      clearTimeout(debounceTimer)
      debounceTimer = setTimeout(() => {
        const bounds = map.getBounds()
        setViewport({
          zoom: map.getZoom(),
          center: [map.getCenter().lng, map.getCenter().lat],
          bbox: [
            bounds.getWest(),
            bounds.getSouth(),
            bounds.getEast(),
            bounds.getNorth(),
          ],
        })
      }, 200)
    }

    map.on('moveend', updateViewport)
    map.once('load', updateViewport)

    return () => {
      clearTimeout(debounceTimer)
      map.remove()
      mapInstance = null
      deckOverlay = null
    }
  }, [setViewport])

  return <div ref={containerRef} style={{ width: '100%', height: '100%' }} />
}

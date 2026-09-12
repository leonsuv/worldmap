import { useEffect, useRef } from 'react'
import * as maplibregl from 'maplibre-gl'
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url'
import 'maplibre-gl/dist/maplibre-gl.css'
import { MapLibreOverlay } from '@deck.gl/maplibre'
import { MAP_STYLES, styleAtlas } from './styles'
import { setMapRuntime } from './runtime'
import { useViewportStore } from '../store/viewport'

maplibregl.setWorkerUrl(workerUrl)

let debounceTimer: ReturnType<typeof setTimeout> | undefined


export default function MapContainer() {
  const containerRef = useRef<HTMLDivElement>(null)
  const setViewport = useViewportStore((s) => s.setViewport)

  useEffect(() => {
    if (!containerRef.current) return

    const map = new maplibregl.Map({
      container: containerRef.current,
      style: MAP_STYLES.dark,
      center: [0, 20],
      zoom: 2,
      hash: true,
      pixelRatio: Math.min(window.devicePixelRatio, 2),
      maxPitch: 65,
      attributionControl: { compact: true },
    })


    const overlay = new MapLibreOverlay({ interleaved: true, layers: [] })
    map.addControl(overlay as unknown as maplibregl.IControl)
    setMapRuntime(map, overlay)

    map.addControl(new maplibregl.ScaleControl({ maxWidth: 100 }), 'bottom-left')

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

    map.on('style.load', () => styleAtlas(map))
    map.on('moveend', updateViewport)
    map.once('load', updateViewport)

    return () => {
      clearTimeout(debounceTimer)
      map.remove()
      setMapRuntime(null, null)
    }
  }, [setViewport])

  return <div className="map-canvas" ref={containerRef} />
}

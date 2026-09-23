import { useEffect, useRef } from 'react'
import * as maplibregl from 'maplibre-gl'
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url'
import 'maplibre-gl/dist/maplibre-gl.css'
import { MapLibreOverlay } from '@deck.gl/maplibre'
import { BASEMAPS, firstSymbolLayer, tuneBasemap } from './basemap'
import { resetOverlays, syncOverlays } from './overlays'
import { setLabelLayer, setRuntime } from './runtime'
import { startDeckRendering, stopDeckRendering } from './deckLayers'
import { attachInteraction } from './interaction'
import { requestRedraw } from './scheduler'
import { useLayers } from '../store/layers'
import { useStatus } from '../store/status'
import { useUi } from '../store/ui'
import { useViewport } from '../store/viewport'
import { staticSignal } from '../data/staticData'

maplibregl.setWorkerUrl(workerUrl)

/** Resolve relative URLs (tile JSON, tiles, API) against the page so workers can fetch them. */
function absolutize(url: string): maplibregl.RequestParameters {
  return { url: url.startsWith('/') ? new URL(url, window.location.origin).href : url }
}

export default function MapView() {
  const container = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!container.current) return
    const ui = useUi.getState()
    let theme = ui.theme
    const map = new maplibregl.Map({
      container: container.current,
      style: BASEMAPS[theme],
      center: [10, 25],
      zoom: 1.6,
      hash: 'map',
      maxPitch: 70,
      attributionControl: { compact: true },
      transformRequest: absolutize,
      fadeDuration: 150,
      pixelRatio: Math.min(window.devicePixelRatio || 1, 2),
    })
    map.dragRotate.enable()
    map.touchZoomRotate.enableRotation()
    map.addControl(new maplibregl.ScaleControl({ maxWidth: 110 }), 'bottom-left')

    // Flat map: deck.gl shares MapLibre's WebGL context so lines sit below labels.
    // Globe: deck.gl draws on its own canvas; interleaved globe rendering is not
    // reliable with this MapLibre version.
    let overlay: MapLibreOverlay | null = null
    let interleaved: boolean | null = null
    const installOverlay = (projection: 'mercator' | 'globe') => {
      const wanted = projection !== 'globe'
      if (overlay && interleaved === wanted) return
      if (overlay) map.removeControl(overlay as unknown as maplibregl.IControl)
      overlay = new MapLibreOverlay({ interleaved: wanted, layers: [] })
      interleaved = wanted
      map.addControl(overlay as unknown as maplibregl.IControl)
      setRuntime(map, overlay)
      requestRedraw()
    }
    installOverlay(ui.projection)

    // deck.gl re-reads the projection on `styledata`, which setProjection does not emit.
    const applyProjection = (type: 'mercator' | 'globe') => {
      try {
        map.setProjection({ type })
      } catch {
        /* projection unsupported: stay on mercator */
      }
      installOverlay(type)
      map.fire('styledata', { dataType: 'style' })
    }

    const sync = () =>
      syncOverlays(map, { enabled: useLayers.getState().enabled, caps: useStatus.getState().caps, theme, trafficStyle: theme })

    map.on('style.load', () => {
      resetOverlays()
      tuneBasemap(map, theme)
      setLabelLayer(firstSymbolLayer(map))
      applyProjection(useUi.getState().projection)
      sync()
      requestRedraw()
    })

    const updateViewport = () => {
      const b = map.getBounds()
      const c = map.getCenter()
      useViewport.getState().setViewport({ zoom: map.getZoom(), center: [c.lng, c.lat], bbox: [b.getWest(), b.getSouth(), b.getEast(), b.getNorth()] })
    }
    map.on('moveend', updateViewport)
    map.once('load', updateViewport)
    map.on('error', event => {
      // Tile and network hiccups are expected; keep the console readable.
      const message = (event as unknown as { error?: { message?: string } }).error?.message ?? ''
      if (!/Failed to fetch|NetworkError|AbortError|204/.test(message)) console.warn('Map error:', message)
    })

    const detach = attachInteraction(map)
    startDeckRendering()

    const unsubscribers = [
      useLayers.subscribe(sync),
      useStatus.subscribe((s, prev) => s.caps !== prev.caps && sync()),
      staticSignal.subscribe(sync),
      useUi.subscribe((s, prev) => {
        if (s.theme !== prev.theme) {
          theme = s.theme
          map.setStyle(BASEMAPS[s.theme])
        }
        if (s.projection !== prev.projection && map.isStyleLoaded()) {
          applyProjection(s.projection)
          requestRedraw()
        }
      }),
    ]

    const resize = new ResizeObserver(() => map.resize())
    resize.observe(container.current)

    return () => {
      unsubscribers.forEach(u => u())
      resize.disconnect()
      detach()
      stopDeckRendering()
      if (overlay) map.removeControl(overlay as unknown as maplibregl.IControl)
      map.remove()
      setRuntime(null, null)
    }
  }, [])

  // MapLibre styles its container as position: relative, so it lives inside an
  // absolutely positioned wrapper that owns the size.
  return (
    <div className="map-canvas" role="region" aria-label="Interactive map">
      <div ref={container} style={{ width: '100%', height: '100%' }} />
    </div>
  )
}

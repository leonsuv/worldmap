import { Component, lazy, Suspense, useEffect, type ReactNode } from 'react'
import TopBar from './ui/TopBar'
import LayerPanel from './ui/LayerPanel'
import MapControls from './ui/MapControls'
import Drawer from './ui/Drawer'
import Legend from './ui/Legend'
import StatusBar from './ui/StatusBar'
import { Notices, PickBanner, Timeline, Tooltip } from './ui/Overlays'
import { useDataFeeds } from './data/controller'
import { useUi } from './store/ui'

const MapView = lazy(() => import('./map/MapView'))

class MapBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  render() {
    if (!this.state.failed) return this.props.children
    return (
      <div className="map-fallback" role="alert">
        <div className="card">
          <strong>The map could not start.</strong>
          <span className="muted">WebGL may be disabled in this browser, or the map engine failed to load.</span>
          <button className="btn primary" onClick={() => window.location.reload()}>
            Reload
          </button>
        </div>
      </div>
    )
  }
}

export default function App() {
  const theme = useUi(s => s.theme)
  const drawerOpen = useUi(s => s.panel !== null)
  const layersOpen = useUi(s => s.layersOpen)
  useDataFeeds()

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme === 'dark' ? '#111a1f' : '#ffffff')
  }, [theme])

  return (
    <div className={`app ${drawerOpen ? 'drawer-open' : ''} ${layersOpen ? '' : 'layers-collapsed'}`}>
      <TopBar />
      <main className="map-area">
        <MapBoundary>
          <Suspense
            fallback={
              <div className="map-fallback" role="status">
                <span className="loading-row">
                  <span className="spinner" /> Loading map…
                </span>
              </div>
            }
          >
            <MapView />
          </Suspense>
        </MapBoundary>
        <LayerPanel />
        <MapControls />
        <Legend />
        <Timeline />
        <Drawer />
        <Tooltip />
        <PickBanner />
        <Notices />
      </main>
      <StatusBar />
    </div>
  )
}

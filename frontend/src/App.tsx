import { lazy, Suspense } from 'react'
import MapErrorBoundary from './ui/MapErrorBoundary'
const MapExperience = lazy(() => import('./map/MapExperience'))
import LayerPanel from './ui/LayerPanel'
import InfoPopup from './ui/InfoPopup'
import FlightDetailPanel from './ui/FlightDetailPanel'
import ShipDetailPanel from './ui/ShipDetailPanel'
import MapControls from './ui/MapControls'
import Attribution from './ui/Attribution'
import Search from './ui/Search'
import WatchlistPanel from './ui/WatchlistPanel'
import EventPanel from './ui/EventPanel'
import AlertPanel from './ui/AlertPanel'
import TimeSlider from './ui/TimeSlider'
import ExportMenu from './ui/ExportMenu'
import BusinessToolbar from './ui/BusinessToolbar'
import WeatherLegend from './ui/WeatherLegend'
import DataLegend from './ui/DataLegend'
import Notice from './ui/Notice'
import MapHeader from './ui/MapHeader'
import './App.css'
import './atlas.css'

function App() {
  return (
    <div className="app">
      <MapErrorBoundary><Suspense fallback={<div className="map-notice" role="status">Preparing your world…</div>}><MapExperience /></Suspense></MapErrorBoundary>
      <MapHeader />
      <Search />
      <LayerPanel />
      <BusinessToolbar />
      <MapControls />
      <InfoPopup />
      <FlightDetailPanel />
      <ShipDetailPanel />
      <WatchlistPanel />
      <EventPanel />
      <AlertPanel />
      <WeatherLegend />
      <DataLegend />
      <TimeSlider />
      <ExportMenu />
      <Attribution />
      <Notice />
    </div>
  )
}

export default App

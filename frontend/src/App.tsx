import MapContainer from './map/MapContainer'
import { useLayers } from './layers/useLayers'
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
import './App.css'

function App() {
  useLayers()
  return (
    <div className="app">
      <MapContainer />
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
      <TimeSlider />
      <ExportMenu />
      <Attribution />
    </div>
  )
}

export default App

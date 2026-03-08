import MapContainer from './map/MapContainer'
import { useLayers } from './layers/useLayers'
import LayerPanel from './ui/LayerPanel'
import InfoPopup from './ui/InfoPopup'
import FlightDetailPanel from './ui/FlightDetailPanel'
import ShipDetailPanel from './ui/ShipDetailPanel'
import MapControls from './ui/MapControls'
import Attribution from './ui/Attribution'
import Search from './ui/Search'
import './App.css'

function App() {
  useLayers()
  return (
    <div className="app">
      <MapContainer />
      <Search />
      <LayerPanel />
      <MapControls />
      <InfoPopup />
      <FlightDetailPanel />
      <ShipDetailPanel />
      <Attribution />
    </div>
  )
}

export default App

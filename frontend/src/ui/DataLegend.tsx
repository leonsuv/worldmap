import { useLayerStore } from '../store/layers'
export default function DataLegend() {
  const layers = useLayerStore()
  if (layers.weather || !(layers.ships || layers.flights || layers.reactors || layers.airports || layers.seaports)) return null
  return <div className="data-legend" aria-label="Map legend"><h3>ON THE MAP</h3>
    {layers.ships && <><div><i style={{background:'#4caf50'}} />Cargo vessels<i style={{background:'#e53935'}} />Tankers</div><div><i style={{background:'#2196f3'}} />Passenger<i style={{background:'#ff9800'}} />Fishing</div></>}
    {layers.flights && <div className="flight-legend"><span>Flight altitude</span><span className="altitude-ramp" /><small>0 — 13,000 m</small></div>}
    {layers.reactors && <div><i style={{background:'#ded68b'}} />Nuclear reactors<small>Size ∝ √capacity</small></div>}
    {layers.airports && <div><i style={{background:'#4682dc'}} />Airports</div>}
    {layers.seaports && <div><i style={{background:'#28b4a0'}} />Seaports</div>}
  </div>
}

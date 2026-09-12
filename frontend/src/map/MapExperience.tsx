import MapContainer from './MapContainer'
import { useLayers } from '../layers/useLayers'
export default function MapExperience() {
  useLayers()
  return <MapContainer />
}

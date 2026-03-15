import { useLayerStore } from '../store/layers'

function WeatherLegend() {
  const weatherEnabled = useLayerStore((s) => s.weather)
  if (!weatherEnabled) return null

  return (
    <div className="weather-legend" aria-live="polite">
      <div className="weather-legend-title">Wind Field</div>
      <div className="weather-legend-sub">Arrow points where wind goes</div>
      <div className="weather-legend-items">
        <div className="weather-legend-item">
          <span className="weather-swatch weather-swatch-1" />
          <span>0-4 m/s</span>
        </div>
        <div className="weather-legend-item">
          <span className="weather-swatch weather-swatch-2" />
          <span>4-8 m/s</span>
        </div>
        <div className="weather-legend-item">
          <span className="weather-swatch weather-swatch-3" />
          <span>8-12 m/s</span>
        </div>
        <div className="weather-legend-item">
          <span className="weather-swatch weather-swatch-4" />
          <span>12-17 m/s</span>
        </div>
        <div className="weather-legend-item">
          <span className="weather-swatch weather-swatch-5" />
          <span>17+ m/s</span>
        </div>
      </div>
    </div>
  )
}

export default WeatherLegend

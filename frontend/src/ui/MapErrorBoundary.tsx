import { Component, type ReactNode } from 'react'
export default class MapErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() {
    if (this.state.failed) return <div className="map-notice" role="alert"><strong>The map could not start.</strong><span>Please check that WebGL is available in your browser.</span><button onClick={() => window.location.reload()}>Reload map</button></div>
    return this.props.children
  }
}

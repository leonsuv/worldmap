import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './styles/app.css'
import App from './App'

// `?debug` exposes the stores for troubleshooting from the browser console.
if (new URLSearchParams(location.search).has('debug')) {
  void Promise.all([import('./store/selection'), import('./store/layers'), import('./store/ui'), import('./map/runtime')]).then(
    ([selection, layers, ui, runtime]) => {
      Object.assign(window, { worldmap: { selection: selection.useSelection, layers: layers.useLayers, ui: ui.useUi, runtime } })
    },
  )
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

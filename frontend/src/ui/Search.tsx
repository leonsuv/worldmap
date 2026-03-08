import { memo, useState, useCallback, useRef } from 'react'
import { Search as SearchIcon } from 'lucide-react'
import { mapInstance } from '../map/MapContainer'

interface NominatimResult {
  display_name: string
  lat: string
  lon: string
}

function Search() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<NominatimResult[]>([])
  const [open, setOpen] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined)

  const doSearch = useCallback(async (q: string) => {
    if (q.length < 2) { setResults([]); return }
    try {
      const url = `https://nominatim.openstreetmap.org/search?q=${encodeURIComponent(q)}&format=json&limit=5`
      const r = await fetch(url, { headers: { 'Accept-Language': 'en' } })
      if (r.ok) {
        const data: NominatimResult[] = await r.json()
        setResults(data)
        setOpen(true)
      }
    } catch { /* network error */ }
  }, [])

  const onChange = (v: string) => {
    setQuery(v)
    clearTimeout(timer.current)
    timer.current = setTimeout(() => doSearch(v), 400)
  }

  const selectResult = (r: NominatimResult) => {
    setOpen(false)
    setQuery(r.display_name)
    mapInstance?.flyTo({ center: [parseFloat(r.lon), parseFloat(r.lat)], zoom: 12 })
  }

  return (
    <div className="search-bar">
      <SearchIcon size={16} className="search-icon" />
      <input
        className="search-input"
        type="text"
        placeholder="Search places…"
        value={query}
        onChange={(e) => onChange(e.target.value)}
        onFocus={() => results.length > 0 && setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 200)}
      />
      {open && results.length > 0 && (
        <ul className="search-results">
          {results.map((r, i) => (
            <li key={i} className="search-result" onMouseDown={() => selectResult(r)}>
              {r.display_name}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

export default memo(Search)

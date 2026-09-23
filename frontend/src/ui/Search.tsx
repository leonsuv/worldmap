import { useEffect, useRef, useState } from 'react'
import { Anchor, Atom, MapPin, Plane, Search as SearchIcon, Ship, X } from 'lucide-react'
import { api, isAbort } from '../lib/api'
import { flyTo } from '../map/runtime'
import { useSelection } from '../store/selection'
import { findFeature } from '../data/staticData'

export interface SearchResult {
  kind: 'airport' | 'seaport' | 'reactor' | 'vessel' | 'place'
  id: string
  name: string
  detail: string
  lat: number
  lon: number
  zoom: number
}

const ICONS = { airport: Plane, seaport: Anchor, reactor: Atom, vessel: Ship, place: MapPin }

function openResult(r: SearchResult) {
  flyTo(r.lon, r.lat, r.zoom)
  const select = useSelection.getState().select
  if (r.kind === 'vessel') select({ kind: 'ship', id: Number(r.id) })
  else if (r.kind === 'airport' || r.kind === 'seaport' || r.kind === 'reactor') {
    const key = r.kind === 'reactor' ? 'reactors' : r.kind === 'airport' ? 'airports' : 'seaports'
    const prop = r.kind === 'airport' ? 'ident' : r.kind === 'seaport' ? 'locode' : 'name'
    const feature = findFeature(key, prop, r.id) ?? findFeature(key, 'name', r.name)
    select({ kind: r.kind, id: r.id, lngLat: [r.lon, r.lat], props: feature?.properties ?? { name: r.name } })
  } else {
    select({ kind: 'place', id: r.id, lngLat: [r.lon, r.lat], props: { name: r.name, detail: r.detail } })
  }
}

export default function Search() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<SearchResult[]>([])
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(-1)
  const [loading, setLoading] = useState(false)
  const [message, setMessage] = useState('')
  const input = useRef<HTMLInputElement>(null)
  const controller = useRef<AbortController | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => {
    const shortcut = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement
      if (e.key === '/' && !['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) {
        e.preventDefault()
        input.current?.focus()
      }
    }
    window.addEventListener('keydown', shortcut)
    return () => {
      window.removeEventListener('keydown', shortcut)
      controller.current?.abort()
      clearTimeout(timer.current)
    }
  }, [])

  const run = async (q: string, places: boolean) => {
    controller.current?.abort()
    const trimmed = q.trim()
    if (trimmed.length < 2) {
      setResults([])
      setMessage('')
      setLoading(false)
      return
    }
    const ac = new AbortController()
    controller.current = ac
    setLoading(true)
    try {
      const data = await api<{ results: SearchResult[]; places_error: string | null }>(
        `/api/search?q=${encodeURIComponent(trimmed)}&places=${places}`,
        { signal: ac.signal },
      )
      if (ac.signal.aborted) return
      setResults(data.results)
      setActive(places && data.results.length ? 0 : -1)
      const hint = places
        ? data.results.length ? '' : 'Nothing found. Try another spelling.'
        : data.results.length ? 'Press Enter to include places worldwide.' : 'Press Enter to search places worldwide.'
      setMessage(data.places_error ?? hint)
      setOpen(true)
    } catch (error) {
      if (!isAbort(error)) {
        setResults([])
        setMessage(error instanceof Error ? error.message : 'Search failed.')
      }
    } finally {
      if (!ac.signal.aborted) setLoading(false)
    }
  }

  const onChange = (value: string) => {
    setQuery(value)
    setOpen(true)
    clearTimeout(timer.current)
    // Local datasets as you type; worldwide places only on Enter (Nominatim policy).
    timer.current = setTimeout(() => void run(value, false), 180)
  }

  const choose = (r: SearchResult) => {
    setOpen(false)
    setQuery(r.name)
    input.current?.blur()
    openResult(r)
  }

  const local = results.filter(r => r.kind !== 'place')
  const places = results.filter(r => r.kind === 'place')
  const ordered = [...local, ...places]

  return (
    <div className="search">
      <label className="search-field">
        {loading ? <span className="spinner" /> : <SearchIcon size={16} />}
        <input
          ref={input}
          role="combobox"
          aria-expanded={open && (ordered.length > 0 || !!message)}
          aria-controls="search-results"
          aria-activedescendant={active >= 0 ? `search-${active}` : undefined}
          aria-label="Search airports, ports, vessels and places"
          placeholder="Search airports, ports, vessels, places…"
          autoComplete="off"
          spellCheck={false}
          value={query}
          onChange={e => onChange(e.target.value)}
          onFocus={() => query.trim().length >= 2 && setOpen(true)}
          onBlur={() => setTimeout(() => setOpen(false), 120)}
          onKeyDown={e => {
            if (e.key === 'ArrowDown' && ordered.length) {
              e.preventDefault()
              setOpen(true)
              setActive(i => (i + 1) % ordered.length)
            } else if (e.key === 'ArrowUp' && ordered.length) {
              e.preventDefault()
              setActive(i => (i - 1 + ordered.length) % ordered.length)
            } else if (e.key === 'Enter') {
              e.preventDefault()
              if (open && active >= 0 && ordered[active]) choose(ordered[active])
              else void run(query, true)
            } else if (e.key === 'Escape') {
              setOpen(false)
              input.current?.blur()
            }
          }}
        />
        {query ? (
          <button className="icon-btn" style={{ width: 24, height: 24 }} aria-label="Clear search" onMouseDown={e => e.preventDefault()} onClick={() => { setQuery(''); setResults([]); setMessage(''); input.current?.focus() }}>
            <X size={14} />
          </button>
        ) : (
          <kbd>/</kbd>
        )}
      </label>
      {open && (ordered.length > 0 || message) && (
        <ul id="search-results" className="search-results" role="listbox">
          {local.length > 0 && <li className="search-group" role="presentation">On the map</li>}
          {ordered.map((r, i) => {
            const Icon = ICONS[r.kind]
            return (
              <li key={`${r.kind}-${r.id}-${i}`} role="presentation">
                {i === local.length && places.length > 0 && <div className="search-group">Places</div>}
                <div
                  id={`search-${i}`}
                  role="option"
                  aria-selected={active === i}
                  className="search-result"
                  onMouseDown={e => e.preventDefault()}
                  onMouseEnter={() => setActive(i)}
                  onClick={() => choose(r)}
                >
                  <span className="kind-icon"><Icon size={15} /></span>
                  <span>
                    <strong>{r.name}</strong>
                    {r.detail && <small>{r.detail}</small>}
                  </span>
                </div>
              </li>
            )
          })}
          {message && <li className="search-hint" role="presentation">{message}</li>}
        </ul>
      )}
    </div>
  )
}

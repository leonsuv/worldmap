import { memo, useState, useEffect, useRef } from 'react'
import { Search as SearchIcon } from 'lucide-react'
import { mapInstance } from '../map/runtime'

interface NominatimResult { display_name: string; lat: string; lon: string }
function Search() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<NominatimResult[]>([])
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [selected, setSelected] = useState(-1)
  const controller = useRef<AbortController | null>(null)
  const input = useRef<HTMLInputElement>(null)
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if (event.key === '/' && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLTextAreaElement)) { event.preventDefault(); input.current?.focus() }
    }
    window.addEventListener('keydown', shortcut)
    return () => { window.removeEventListener('keydown', shortcut); controller.current?.abort() }
  }, [])
  const submit = async () => {
    controller.current?.abort()
    const q = query.trim()
    if (q.length < 2) { setResults([]); setOpen(false); return }
    const ac = new AbortController()
    controller.current = ac
    setLoading(true); setError(''); setOpen(true); setSelected(-1)
    try {
      const response = await fetch(`/api/search?q=${encodeURIComponent(q)}`, { signal: ac.signal })
      if (!response.ok) throw new Error('Search unavailable. Please try again.')
      const data: NominatimResult[] = await response.json()
      if (!ac.signal.aborted) { setResults(data); if (!data.length) setError('No places found. Try a different name.') }
    } catch (error) {
      if (!ac.signal.aborted) { setResults([]); setError(error instanceof Error ? error.message : 'Search unavailable.') }
    } finally { if (!ac.signal.aborted) setLoading(false) }
  }
  const selectResult = (result: NominatimResult) => {
    controller.current?.abort(); setLoading(false); setOpen(false); setQuery(result.display_name)
    mapInstance?.flyTo({ center: [Number(result.lon), Number(result.lat)], zoom: 11, duration: 1200 })
  }
  return <div className="search-bar">
    <SearchIcon size={16} className="search-icon" />
    <input ref={input} className="search-input" role="combobox" aria-label="Search places" aria-expanded={open} aria-controls="place-results" aria-activedescendant={selected >= 0 ? `place-${selected}` : undefined} autoComplete="off" placeholder="Search a place, then press Enter…" value={query}
      onChange={e => { controller.current?.abort(); setLoading(false); setQuery(e.target.value); setResults([]); setOpen(false); setSelected(-1) }}
      onFocus={() => results.length > 0 && setOpen(true)} onBlur={() => setOpen(false)}
      onKeyDown={e => {
        if (e.key === 'Escape') { controller.current?.abort(); setLoading(false); setOpen(false); input.current?.blur() }
        if (e.key === 'ArrowDown' && results.length) { e.preventDefault(); setOpen(true); setSelected(i => (i + 1) % results.length) }
        if (e.key === 'ArrowUp' && results.length) { e.preventDefault(); setSelected(i => (i - 1 + results.length) % results.length) }
        if (e.key === 'Enter') { e.preventDefault(); if (open && selected >= 0 && results[selected]) selectResult(results[selected]); else void submit() }
      }} />
    <kbd className="search-key">↵</kbd>
    {open && <ul id="place-results" className="search-results" role="listbox" aria-label="Places">
      {loading ? <li role="presentation" className="search-message">Searching places…</li> : error ? <li role="presentation" className="search-message">{error}</li> : results.map((r, i) => <li id={`place-${i}`} key={`${r.lat},${r.lon}`} role="option" aria-selected={selected === i} className={`search-result ${selected === i ? 'selected' : ''}`} onMouseDown={e => { e.preventDefault(); selectResult(r) }}>{r.display_name}</li>)}
    </ul>}
  </div>
}
export default memo(Search)

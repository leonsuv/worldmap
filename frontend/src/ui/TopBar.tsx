import { useEffect, useRef, useState } from 'react'
import { Bell, Download, FileSpreadsheet, FileText, Globe2, ListChecks, TriangleAlert } from 'lucide-react'
import Search from './Search'
import { useUi, type Panel } from '../store/ui'
import { useAlerts } from '../store/alerts'
import { api } from '../lib/api'
import { notify } from '../store/notice'
import { reportMarkdown, type Report } from '../lib/report'
import { getMap } from '../map/runtime'

function PanelButton({ panel, label, icon, badge }: { panel: Panel; label: string; icon: React.ReactNode; badge?: number }) {
  const open = useUi(s => s.panel === panel)
  const toggle = useUi(s => s.togglePanel)
  return (
    <button className="topbar-btn" aria-pressed={open} title={label} onClick={() => toggle(panel)}>
      {icon}
      <span className="label">{label}</span>
      {!!badge && <span className="badge" aria-label={`${badge} unread`}>{badge > 99 ? '99+' : badge}</span>}
    </button>
  )
}

function download(name: string, text: string, type: string) {
  const url = URL.createObjectURL(new Blob([text], { type }))
  const a = document.createElement('a')
  a.href = url
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

function ExportMenu() {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const close = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && setOpen(false)
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    window.addEventListener('pointerdown', close)
    window.addEventListener('keydown', esc)
    return () => {
      window.removeEventListener('pointerdown', close)
      window.removeEventListener('keydown', esc)
    }
  }, [open])
  const csv = (type: string) => {
    const a = document.createElement('a')
    a.href = `/api/export/csv?type=${type}`
    a.download = ''
    a.click()
    setOpen(false)
  }
  const report = async () => {
    setOpen(false)
    try {
      const data = await api<Report>('/api/export/report')
      download(`situation-report-${new Date().toISOString().slice(0, 10)}.md`, reportMarkdown(data), 'text/markdown')
    } catch (error) {
      notify(error instanceof Error ? error.message : 'Report unavailable')
    }
  }
  return (
    <div ref={ref} style={{ position: 'relative' }}>
      <button className="topbar-btn" aria-haspopup="menu" aria-expanded={open} title="Export data" onClick={() => setOpen(o => !o)}>
        <Download size={17} />
        <span className="label">Export</span>
      </button>
      {open && (
        <div className="menu surface" role="menu">
          <div className="menu-label">CSV</div>
          {[
            ['ships', 'Live vessels'],
            ['events', 'Events'],
            ['alerts', 'Alerts'],
            ['watchlist', 'Watchlist'],
          ].map(([type, label]) => (
            <button key={type} role="menuitem" onClick={() => csv(type)}>
              <FileSpreadsheet size={15} /> {label}
            </button>
          ))}
          <hr />
          <button role="menuitem" onClick={report}>
            <FileText size={15} /> Situation report (Markdown)
          </button>
        </div>
      )}
    </div>
  )
}

export default function TopBar() {
  const alerts = useAlerts(s => s.count)
  return (
    <header className="topbar">
      <button className="brand" title="Show the whole world" onClick={() => getMap()?.flyTo({ center: [10, 25], zoom: 1.6, bearing: 0, pitch: 0, duration: 1400 })}>
        <span className="brand-mark">
          <Globe2 size={19} strokeWidth={1.6} />
        </span>
        <span>
          <span className="brand-name">
            WORLDMAP<span>.</span>
          </span>
          <span className="brand-tagline">Global infrastructure atlas</span>
        </span>
      </button>
      <Search />
      <nav className="topbar-actions" aria-label="Tools">
        <PanelButton panel="watchlist" label="Watchlist" icon={<ListChecks size={17} />} />
        <PanelButton panel="events" label="Events" icon={<TriangleAlert size={17} />} />
        <PanelButton panel="alerts" label="Alerts" icon={<Bell size={17} />} badge={alerts} />
        <ExportMenu />
      </nav>
    </header>
  )
}

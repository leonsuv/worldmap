import { memo, useState } from 'react'
import { Download, FileText, FileSpreadsheet } from 'lucide-react'

function ExportMenu() {
  const [open, setOpen] = useState(false)
  const [generating, setGenerating] = useState(false)

  const downloadCsv = (type: string) => {
    window.open(`/api/export/csv?type=${type}`, '_blank')
    setOpen(false)
  }

  const downloadReport = async () => {
    setGenerating(true)
    try {
      const r = await fetch('/api/export/report')
      if (!r.ok) return
      const report = await r.json()
      // Generate a text-based situation report
      const lines = [
        `SITUATION REPORT`,
        `Generated: ${new Date(report.generated_at * 1000).toLocaleString()}`,
        ``,
        `OVERVIEW`,
        `  Total Ships Tracked: ${report.total_ships}`,
        `  Total Events: ${report.total_events}`,
        `  Unacknowledged Alerts: ${report.unacknowledged_alerts}`,
        `  Watchlist Items: ${report.watchlist_count}`,
        ``,
        `ACTIVE EVENTS`,
      ]
      for (const ev of report.active_events) {
        lines.push(`  [${ev.event_type.toUpperCase()}] ${ev.name}`)
        lines.push(`    Location: ${ev.lat.toFixed(4)}, ${ev.lon.toFixed(4)} — Radius: ${ev.radius_km} km`)
        lines.push(`    Affected: ${ev.affected_count} assets`)
        if (ev.description) lines.push(`    Description: ${ev.description}`)
        lines.push('')
      }
      if (report.active_events.length === 0) lines.push('  No active events')

      const blob = new Blob([lines.join('\n')], { type: 'text/plain' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `situation-report-${new Date().toISOString().slice(0, 10)}.txt`
      a.click()
      URL.revokeObjectURL(url)
    } finally {
      setGenerating(false)
      setOpen(false)
    }
  }

  return (
    <div className="export-wrap">
      <button className="export-toggle" onClick={() => setOpen(o => !o)} title="Export Data">
        <Download size={16} />
      </button>
      {open && (
        <div className="export-menu">
          <div className="export-menu-title">Export</div>
          <button className="export-opt" onClick={() => downloadCsv('ships')}>
            <FileSpreadsheet size={14} /> Ships CSV
          </button>
          <button className="export-opt" onClick={() => downloadCsv('events')}>
            <FileSpreadsheet size={14} /> Events CSV
          </button>
          <button className="export-opt" onClick={() => downloadCsv('alerts')}>
            <FileSpreadsheet size={14} /> Alerts CSV
          </button>
          <button className="export-opt" onClick={() => downloadCsv('watchlist')}>
            <FileSpreadsheet size={14} /> Watchlist CSV
          </button>
          <hr className="export-sep" />
          <button className="export-opt" onClick={downloadReport} disabled={generating}>
            <FileText size={14} /> {generating ? 'Generating…' : 'Situation Report'}
          </button>
        </div>
      )}
    </div>
  )
}

export default memo(ExportMenu)

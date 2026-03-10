import { memo, useEffect } from 'react'
import { useAlertStore } from '../store/alerts'
import { Bell, Check, CheckCheck, X } from 'lucide-react'

function AlertPanel() {
  const { alerts, count, open, toggle, fetch, ack, ackAll } = useAlertStore()

  // Poll alert count every 30s
  useEffect(() => {
    const { fetchCount } = useAlertStore.getState()
    fetchCount()
    const id = setInterval(fetchCount, 30_000)
    return () => clearInterval(id)
  }, [])

  useEffect(() => { if (open) fetch() }, [open, fetch])

  return (
    <>
      {/* Badge button */}
      <button className="alert-badge" onClick={toggle}>
        <Bell size={16} />
        {count > 0 && <span className="alert-badge-count">{count > 99 ? '99+' : count}</span>}
      </button>

      {/* Dropdown panel */}
      {open && (
        <div className="alert-panel">
          <div className="alert-header">
            <h2 className="alert-title">Alerts</h2>
            {alerts.some(a => !a.acknowledged) && (
              <button className="alert-ack-all" onClick={ackAll}>
                <CheckCheck size={14} /> Ack All
              </button>
            )}
            <button className="alert-close" onClick={toggle}><X size={16} /></button>
          </div>

          <div className="alert-list">
            {alerts.map(a => (
              <div key={a.id} className={`alert-item ${a.acknowledged ? 'acked' : ''} sev-${a.severity}`}>
                <div className="alert-item-body">
                  <span className="alert-item-title">{a.title}</span>
                  <span className="alert-item-msg">{a.message}</span>
                  <span className="alert-item-time">{new Date(a.created_at * 1000).toLocaleString()}</span>
                </div>
                {!a.acknowledged && (
                  <button className="alert-item-ack" onClick={() => ack(a.id)} title="Acknowledge">
                    <Check size={14} />
                  </button>
                )}
              </div>
            ))}
            {alerts.length === 0 && <div className="alert-empty">No alerts</div>}
          </div>
        </div>
      )}
    </>
  )
}

export default memo(AlertPanel)

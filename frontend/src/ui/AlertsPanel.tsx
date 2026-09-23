import { useEffect } from 'react'
import { Bell, Check, CheckCheck } from 'lucide-react'
import { DrawerShell, Empty, Loading } from './common'
import { useAlerts, type Alert } from '../store/alerts'
import { useEvents } from '../store/events'
import { useUi } from '../store/ui'
import { fitCircle } from '../map/runtime'
import { fmtDateTime, timeAgo } from '../lib/format'

export default function AlertsPanel() {
  const { alerts, loading, fetch, ack, ackAll } = useAlerts()
  const close = useUi(s => s.closePanel)
  useEffect(() => {
    void fetch()
  }, [fetch])

  const openEvent = (a: Alert) => {
    if (a.event_id == null) return
    const event = useEvents.getState().events.find(e => e.id === a.event_id)
    useUi.getState().openPanel('events')
    useEvents.getState().select(a.event_id)
    if (event) fitCircle(event.lat, event.lon, event.radius_km)
  }

  const unread = alerts.filter(a => !a.acknowledged).length
  return (
    <DrawerShell
      icon={<Bell size={20} />}
      title="Alerts"
      subtitle={unread ? `${unread} unacknowledged` : 'All caught up'}
      onClose={close}
      footer={
        unread > 0 && (
          <button className="btn block" onClick={() => void ackAll()}>
            <CheckCheck size={15} /> Acknowledge all
          </button>
        )
      }
    >
      {loading && !alerts.length ? (
        <Loading />
      ) : alerts.length === 0 ? (
        <Empty icon={<Bell size={26} />}>
          No alerts. Alerts appear when a watched vessel, site or area falls inside an active event.
        </Empty>
      ) : (
        <ul className="list">
          {alerts.map(a => (
            <li key={a.id} className={`alert-item ${a.severity} ${a.acknowledged ? 'acked' : ''}`}>
              <button style={{ textAlign: 'left' }} onClick={() => openEvent(a)} title="Show the event">
                <strong>{a.title}</strong>
                <p>{a.message}</p>
                <time dateTime={new Date(a.created_at * 1000).toISOString()} title={fmtDateTime(a.created_at)}>
                  {a.severity === 'critical' ? 'Critical · ' : ''}
                  {timeAgo(a.created_at)}
                </time>
              </button>
              {!a.acknowledged ? (
                <button className="icon-btn" aria-label="Acknowledge" title="Acknowledge" onClick={() => void ack(a.id)}>
                  <Check size={16} />
                </button>
              ) : (
                <span />
              )}
            </li>
          ))}
        </ul>
      )}
    </DrawerShell>
  )
}

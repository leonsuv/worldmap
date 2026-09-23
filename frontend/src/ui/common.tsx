import type { ReactNode } from 'react'
import { X } from 'lucide-react'

export function DrawerShell({ icon, title, subtitle, onClose, children, footer, iconColor }: {
  icon: ReactNode
  title: ReactNode
  subtitle?: ReactNode
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  iconColor?: string
}) {
  return (
    <aside className="drawer surface" aria-label={typeof title === 'string' ? title : 'Details'}>
      <header className="drawer-head">
        <span className="drawer-icon" style={iconColor ? { color: iconColor } : undefined}>{icon}</span>
        <div className="drawer-title">
          <h2>{title}</h2>
          {subtitle && <p>{subtitle}</p>}
        </div>
        <button className="icon-btn" aria-label="Close panel" title="Close (Esc)" onClick={onClose}>
          <X size={18} />
        </button>
      </header>
      <div className="drawer-body">{children}</div>
      {footer && <footer className="drawer-foot">{footer}</footer>}
    </aside>
  )
}

export function Section({ title, children, action }: { title: string; children: ReactNode; action?: ReactNode }) {
  return (
    <section className="section">
      <div className="row" style={{ justifyContent: 'space-between' }}>
        <h3>{title}</h3>
        {action}
      </div>
      {children}
    </section>
  )
}

export type KvRow = [label: string, value: ReactNode | null | undefined]

/** Label/value list; empty values are left out. */
export function KeyValues({ rows }: { rows: KvRow[] }) {
  const visible = rows.filter(([, v]) => v !== null && v !== undefined && v !== '' && v !== '—')
  if (!visible.length) return null
  return (
    <dl className="kv">
      {visible.map(([label, value]) => (
        <div key={label} style={{ display: 'contents' }}>
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  )
}

export function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="stat">
      <b>{value}</b>
      <span>{label}</span>
    </div>
  )
}

export function Loading({ label = 'Loading…' }: { label?: string }) {
  return (
    <div className="loading-row" role="status">
      <span className="spinner" />
      {label}
    </div>
  )
}

export function Empty({ icon, children }: { icon?: ReactNode; children: ReactNode }) {
  return (
    <div className="empty">
      {icon}
      <div>{children}</div>
    </div>
  )
}

export function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {children} ↗
    </a>
  )
}

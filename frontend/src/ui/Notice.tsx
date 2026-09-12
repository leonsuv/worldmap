import { X, AlertCircle } from 'lucide-react'
import { useNoticeStore } from '../store/notice'
export default function Notice() {
  const { message, dismiss } = useNoticeStore()
  if (!message) return null
  return <div className="app-notice" role="alert"><AlertCircle size={17} /><span>{message}</span><button aria-label="Dismiss notification" onClick={dismiss}><X size={16} /></button></div>
}

import { create } from 'zustand'

export interface Notice {
  id: number
  message: string
  tone: 'error' | 'info'
}

let next = 1

export const useNotices = create<{ notices: Notice[]; push: (message: string, tone?: Notice['tone']) => void; dismiss: (id: number) => void }>(set => ({
  notices: [],
  push: (message, tone = 'error') => {
    const id = next++
    set(s => ({ notices: [...s.notices.filter(n => n.message !== message), { id, message, tone }].slice(-3) }))
    setTimeout(() => set(s => ({ notices: s.notices.filter(n => n.id !== id) })), tone === 'error' ? 8000 : 4000)
  },
  dismiss: id => set(s => ({ notices: s.notices.filter(n => n.id !== id) })),
}))

export const notify = (message: string, tone: Notice['tone'] = 'error') => useNotices.getState().push(message, tone)

/** Run an action and show its error as a notice. Returns true on success. */
export async function attempt(action: () => Promise<unknown>): Promise<boolean> {
  try {
    await action()
    return true
  } catch (error) {
    notify(error instanceof Error ? error.message : 'Something went wrong. Please try again.')
    return false
  }
}

import { create } from 'zustand'
import { useUi } from './ui'

export type Selection =
  | { kind: 'flight'; id: string }
  | { kind: 'ship'; id: number }
  | {
      kind: 'airport' | 'seaport' | 'reactor' | 'aton' | 'weather' | 'hvline' | 'pipeline' | 'grid' | 'place'
      id: string
      lngLat: [number, number]
      props: Record<string, unknown>
    }

interface SelectionState {
  selected: Selection | null
  select: (selection: Selection) => void
  clear: () => void
}

export function sameSelection(a: Selection | null, b: Selection | null): boolean {
  return !!a && !!b && a.kind === b.kind && a.id === b.id
}

export const useSelection = create<SelectionState>(set => ({
  selected: null,
  select: selection => {
    set({ selected: selection })
    useUi.getState().openPanel('details')
  },
  clear: () => {
    set({ selected: null })
    if (useUi.getState().panel === 'details') useUi.getState().closePanel()
  },
}))

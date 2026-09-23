/** Error with the HTTP status and the server's `{ "error": ... }` message. */
export class ApiError extends Error {
  readonly status: number
  constructor(message: string, status: number) {
    super(message)
    this.status = status
  }
}

export function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}

/** Fetch JSON from the backend; non-2xx responses become an ApiError. */
export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response
  try {
    response = await fetch(path, init)
  } catch (error) {
    if (isAbort(error)) throw error
    throw new ApiError('Cannot reach the WorldMap server. Is the backend running?', 0)
  }
  if (!response.ok) {
    let message = `Request failed (HTTP ${response.status})`
    try {
      const body = await response.json()
      if (body && typeof body.error === 'string') message = body.error
    } catch {
      /* not JSON */
    }
    throw new ApiError(message, response.status)
  }
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}

export function postJson<T>(path: string, body: unknown): Promise<T> {
  return api<T>(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
}

/** Convert `{fields, rows}` responses into objects. */
export function rowsToObjects<T>(fields: readonly string[], rows: readonly unknown[][]): T[] {
  return rows.map(row => {
    const out: Record<string, unknown> = {}
    for (let i = 0; i < fields.length; i++) out[fields[i]] = row[i] ?? null
    return out as T
  })
}

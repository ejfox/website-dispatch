import { describe, expect, it } from 'vitest'
import { ensureShape, isMasked } from './useSettings'
import type { AppConfig } from '../types'

describe('settings helpers', () => {
  it('recognizes the backend mask ("••••" + last 4) and nothing else', () => {
    expect(isMasked('••••1234')).toBe(true)
    expect(isMasked('••••')).toBe(true)
    expect(isMasked('sk-real-key')).toBe(false)
    expect(isMasked('')).toBe(false)
    expect(isMasked(null)).toBe(false)
  })

  it('fills in missing media sections so panes can bind to them', () => {
    const c = ensureShape({ media: { primary: 'cloudinary', mirrors: [], cloudinary: null, r2: null } } as unknown as AppConfig)
    expect(c.media.cloudinary).toEqual({ cloud_name: '', api_key: '', api_secret: '' })
    expect(c.media.r2?.bucket).toBe('')
    const bare = ensureShape({} as AppConfig)
    expect(bare.media.primary).toBe('cloudinary')
  })
})

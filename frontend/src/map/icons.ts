/** One canvas atlas with all deck.gl icons, drawn in white and tinted per feature. */

const CELL = 64

export type IconName = 'plane' | 'ship' | 'shipStill' | 'wind'

type Mapping = Record<IconName, { x: number; y: number; width: number; height: number; anchorX: number; anchorY: number; mask: true }>

const ORDER: IconName[] = ['plane', 'ship', 'shipStill', 'wind']

function draw(ctx: CanvasRenderingContext2D, name: IconName) {
  ctx.fillStyle = '#fff'
  ctx.strokeStyle = '#fff'
  ctx.lineJoin = 'round'
  ctx.lineCap = 'round'
  switch (name) {
    case 'plane': {
      // Airliner seen from above, nose up.
      ctx.beginPath()
      ctx.moveTo(32, 3)
      ctx.bezierCurveTo(35.5, 3, 36, 9, 36, 14)
      ctx.lineTo(36, 23)
      ctx.lineTo(60, 36)
      ctx.lineTo(60, 41)
      ctx.lineTo(36, 34)
      ctx.lineTo(35.5, 50)
      ctx.lineTo(44, 56)
      ctx.lineTo(44, 60)
      ctx.lineTo(32, 57)
      ctx.lineTo(20, 60)
      ctx.lineTo(20, 56)
      ctx.lineTo(28.5, 50)
      ctx.lineTo(28, 34)
      ctx.lineTo(4, 41)
      ctx.lineTo(4, 36)
      ctx.lineTo(28, 23)
      ctx.lineTo(28, 14)
      ctx.bezierCurveTo(28, 9, 28.5, 3, 32, 3)
      ctx.closePath()
      ctx.fill()
      break
    }
    case 'ship': {
      // Hull pointing in the direction of travel.
      ctx.beginPath()
      ctx.moveTo(32, 4)
      ctx.lineTo(45, 30)
      ctx.lineTo(43, 58)
      ctx.lineTo(32, 52)
      ctx.lineTo(21, 58)
      ctx.lineTo(19, 30)
      ctx.closePath()
      ctx.fill()
      break
    }
    case 'shipStill': {
      ctx.beginPath()
      ctx.arc(32, 32, 15, 0, Math.PI * 2)
      ctx.fill()
      break
    }
    case 'wind': {
      ctx.lineWidth = 6
      ctx.beginPath()
      ctx.moveTo(32, 58)
      ctx.lineTo(32, 10)
      ctx.moveTo(17, 25)
      ctx.lineTo(32, 9)
      ctx.lineTo(47, 25)
      ctx.stroke()
      break
    }
  }
}

let atlas: { url: string; mapping: Mapping } | null = null

export function iconAtlas(): { url: string; mapping: Mapping } {
  if (atlas) return atlas
  const canvas = document.createElement('canvas')
  canvas.width = CELL * ORDER.length
  canvas.height = CELL
  const ctx = canvas.getContext('2d')!
  const mapping = {} as Mapping
  ORDER.forEach((name, i) => {
    ctx.save()
    ctx.translate(i * CELL, 0)
    draw(ctx, name)
    ctx.restore()
    mapping[name] = { x: i * CELL, y: 0, width: CELL, height: CELL, anchorX: CELL / 2, anchorY: CELL / 2, mask: true }
  })
  atlas = { url: canvas.toDataURL(), mapping }
  return atlas
}

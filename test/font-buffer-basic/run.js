const { promises } = require('fs')
const { join } = require('path')
const { Resvg } = require('../../index.js')

async function main() {
  const svg = await promises.readFile(join(__dirname, 'input.svg'), 'utf8')
  const fontFiles = [
    { name: 'Lobster', file: 'Lobster-Regular.ttf' },
    { name: 'Jost-700', file: 'Jost-700.ttf' },
  ]
  const fontBuffers = await Promise.all(
    fontFiles.map(async ({ name, file }) => {
      const buffer = await promises.readFile(join(__dirname, 'fonts', file))
      return { fontName: name, buffer: Array.from(buffer) }
    }),
  )

  console.log(fontBuffers.map((f) => ({ name: f.fontName, size: f.buffer.length })))

  const resvg = new Resvg(svg, {
    background: '#fff',
    font: {
      loadSystemFonts: false,
      fontBuffers,
      defaultFontFamily: 'Lobster',
    },
    logLevel: 'debug',
  })

  const pngData = resvg.render()
  const pngBuffer = pngData.asPng()
  await promises.writeFile(join(__dirname, 'output.png'), Buffer.from(pngBuffer))
  console.log('💾 output.png создан')
}

main().catch(console.error)

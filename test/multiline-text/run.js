const { promises } = require('fs')
const { join } = require('path')
const { Resvg } = require('../../index.js')

async function main() {
  const svg = await promises.readFile(join(__dirname, 'input.svg'), 'utf8')
  const fontFiles = [{ name: 'Jost-700', file: 'Jost-700.ttf' }]
  const fontBuffers = await Promise.all(
    fontFiles.map(async ({ name, file }) => {
      const buffer = await promises.readFile(join(__dirname, 'fonts', file))
      return { fontName: name, buffer: Array.from(buffer) }
    }),
  )

  const resvg = new Resvg(svg, {
    background: '#fff',
    font: {
      loadSystemFonts: false,
      fontBuffers,
    },
    logLevel: 'debug',
    textLayout: {
      'main-text': {
        width: 500,
        lineHeight: 24,
        // maxlines: 3,
        fontFamily: 'Jost-700',
        opacity: 0.5,
        fontSize: 24,
        fill: '#006699',
        textAlign: 'right',
        letterSpacing: 2.5,
      },
    },
  })

  const pngData = resvg.render()
  const pngBuffer = pngData.asPng()
  await promises.writeFile(join(__dirname, 'output.png'), Buffer.from(pngBuffer))
  console.log(`💾 output.png создан ${join(__dirname, 'output.png')}`)
}

main().catch(console.error)

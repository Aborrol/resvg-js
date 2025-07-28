#!/usr/bin/env node

const fs = require('fs')
const path = require('path')
const glob = require('glob')

// Read main package.json
const mainPackage = JSON.parse(fs.readFileSync('./package.json', 'utf8'))
const version = mainPackage.version

console.log(`Updating all platform packages to version: ${version}`)

// Find all platform package.json files
const platformPackages = glob.sync('npm/*/package.json')

platformPackages.forEach((filePath) => {
  console.log(`Updating ${filePath}...`)

  const packageJson = JSON.parse(fs.readFileSync(filePath, 'utf8'))

  // Update version
  packageJson.version = version

  // Update repository
  packageJson.repository = 'https://github.com/Aborrol/resvg-js'

  // Write back
  fs.writeFileSync(filePath, JSON.stringify(packageJson, null, 2) + '\n')
})

console.log(`✅ Updated ${platformPackages.length} platform packages to version ${version}`)
console.log(`ℹ️  Note: optionalDependencies are not added to main package.json to avoid CI issues`)

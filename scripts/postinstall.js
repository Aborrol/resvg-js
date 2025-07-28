#!/usr/bin/env node

const { getPlatformPackage } = require('./install-platform')
const { execSync } = require('child_process')

// Only run in production installs, not in development
if (process.env.NODE_ENV === 'development') {
  console.log('Skipping platform package installation in development mode')
  process.exit(0)
}

try {
  const packageName = getPlatformPackage()
  console.log(`Checking if platform package is installed: ${packageName}`)

  // Check if the package is already installed
  try {
    require.resolve(packageName)
    console.log(`✅ Platform package ${packageName} is already installed`)
  } catch (e) {
    console.log(`Installing platform package: ${packageName}`)
    execSync(`npm install ${packageName}`, {
      stdio: 'inherit',
      cwd: process.cwd(),
    })
    console.log(`✅ Successfully installed ${packageName}`)
  }
} catch (error) {
  console.warn(`⚠️  Could not install platform package: ${error.message}`)
  console.log('You may need to install it manually:')
  console.log(`npm install ${getPlatformPackage()}`)
  // Don't fail the install, just warn
  process.exit(0)
}

#!/usr/bin/env node

const { platform, arch } = process
const { execSync } = require('child_process')

// Map platform and architecture to package name
const platformPackages = {
  darwin: {
    x64: '@aborrol/resvg-js-tolty-darwin-x64',
    arm64: '@aborrol/resvg-js-tolty-darwin-arm64',
  },
  linux: {
    x64: '@aborrol/resvg-js-tolty-linux-x64-gnu',
    arm64: '@aborrol/resvg-js-tolty-linux-arm64-gnu',
    arm: '@aborrol/resvg-js-tolty-linux-arm-gnueabihf',
  },
  win32: {
    x64: '@aborrol/resvg-js-tolty-win32-x64-msvc',
    ia32: '@aborrol/resvg-js-tolty-win32-ia32-msvc',
    arm64: '@aborrol/resvg-js-tolty-win32-arm64-msvc',
  },
  android: {
    arm64: '@aborrol/resvg-js-tolty-android-arm64',
    arm: '@aborrol/resvg-js-tolty-android-arm-eabi',
  },
}

function getPlatformPackage() {
  const platformPackagesForOS = platformPackages[platform]
  if (!platformPackagesForOS) {
    throw new Error(`Unsupported platform: ${platform}`)
  }

  const packageName = platformPackagesForOS[arch]
  if (!packageName) {
    throw new Error(`Unsupported architecture on ${platform}: ${arch}`)
  }

  return packageName
}

function installPlatformPackage() {
  try {
    const packageName = getPlatformPackage()
    console.log(`Installing platform-specific package: ${packageName}`)

    execSync(`npm install ${packageName}`, {
      stdio: 'inherit',
      cwd: process.cwd(),
    })

    console.log(`✅ Successfully installed ${packageName}`)
  } catch (error) {
    console.error(`❌ Failed to install platform package: ${error.message}`)
    console.log('\nPlease install the appropriate platform package manually:')
    console.log(`npm install ${getPlatformPackage()}`)
    process.exit(1)
  }
}

// Run if called directly
if (require.main === module) {
  installPlatformPackage()
}

module.exports = { getPlatformPackage, installPlatformPackage }

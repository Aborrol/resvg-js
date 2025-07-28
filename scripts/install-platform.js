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
  // Handle special cases for different platforms
  let os = platform
  let architecture = arch

  // Debug logging
  console.log(`🔍 Detected platform: ${platform}, architecture: ${arch}`)

  // Handle Windows special case - we need to determine if it's MSVC
  if (platform === 'win32') {
    // For Windows, we assume MSVC toolchain (which is what we build with)
    // x64 -> x64-msvc, ia32 -> ia32-msvc
    if (arch === 'x64') {
      architecture = 'x64' // Keep as x64 for win32-x64-msvc
    } else if (arch === 'ia32') {
      architecture = 'ia32' // Keep as ia32 for win32-ia32-msvc
    } else if (arch === 'arm64') {
      architecture = 'arm64' // Keep as arm64 for win32-arm64-msvc
    }
  }

  // Handle Linux special case - we need to determine if it's GNU or MUSL
  if (platform === 'linux') {
    // For Linux, we assume GNU toolchain (which is what we build with)
    // x64 -> x64-gnu, arm64 -> arm64-gnu, arm -> arm-gnueabihf
    if (arch === 'x64') {
      architecture = 'x64' // Keep as x64 for linux-x64-gnu
    } else if (arch === 'arm64') {
      architecture = 'arm64' // Keep as arm64 for linux-arm64-gnu
    } else if (arch === 'arm') {
      architecture = 'arm' // Keep as arm for linux-arm-gnueabihf
    }
  }

  console.log(`🔍 Mapped to OS: ${os}, architecture: ${architecture}`)

  const platformPackagesForOS = platformPackages[os]
  if (!platformPackagesForOS) {
    throw new Error(`Unsupported platform: ${platform}`)
  }

  const packageName = platformPackagesForOS[architecture]
  if (!packageName) {
    throw new Error(`Unsupported architecture on ${platform}: ${arch} (mapped to ${architecture})`)
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

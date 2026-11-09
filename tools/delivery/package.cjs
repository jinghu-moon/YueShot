// Windows-only packager entry point. Use the pinned native API directly;
// Electron discovery and JavaScript config-merging plugins are not used here.
const fs = require('node:fs');
const path = require('node:path');

try {
  if (process.platform !== 'win32' || process.arch !== 'x64') {
    throw new Error('YueShot delivery requires Windows x64 Node.js');
  }
  if (process.argv.length !== 4 || process.argv[2] !== '--config') {
    throw new Error('Usage: node tools/delivery/package.cjs --config <packager.json>');
  }
  const pkg = require('@crabnebula/packager-win32-x64-msvc/package.json');
  if (pkg.version !== '0.11.2') throw new Error('Unexpected native packager version');
  const config = JSON.parse(fs.readFileSync(path.resolve(process.argv[3]), 'utf8'));
  const packager = require('@crabnebula/packager-win32-x64-msvc');
  packager.initTracingSubscriber(0);
  packager.packageApp(JSON.stringify(config));
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}

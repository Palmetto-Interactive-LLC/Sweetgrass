#!/usr/bin/env node

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const PORT = 3008;
const URL = `http://localhost:${PORT}`;

// Determine binary name based on platform
function getBinaryName() {
  const platform = process.platform;
  const ext = platform === 'win32' ? '.exe' : '';
  return `beads-server${ext}`;
}

// Find the binary in the package directory
function findBinary() {
  const binaryName = getBinaryName();
  const binDir = __dirname;
  const binaryPath = path.join(binDir, binaryName);

  if (!fs.existsSync(binaryPath)) {
    console.error(`Error: Binary not found at ${binaryPath}`);
    console.error('Please try reinstalling the package: npm install -g sweetgrass');
    process.exit(1);
  }

  return binaryPath;
}

// Open browser using platform-specific commands without shell interpolation.
function openBrowser() {
  const platform = process.platform;
  let command;
  let args;

  if (platform === 'darwin') {
    command = 'open';
    args = [URL];
  } else if (platform === 'win32') {
    command = 'cmd.exe';
    args = ['/c', 'start', '', URL];
  } else {
    command = 'xdg-open';
    args = [URL];
  }

  const browserProcess = spawn(command, args, {
    detached: true,
    stdio: 'ignore'
  });

  browserProcess.on('error', () => {
    console.log(`Could not open browser automatically. Please navigate to: ${URL}`);
  });

  browserProcess.unref();
}

function main() {
  const binaryPath = findBinary();

  console.log('Starting Sweetgrass server...');

  // Spawn the server process
  const serverProcess = spawn(binaryPath, [], {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, PORT: PORT.toString() }
  });

  // Forward stdout
  serverProcess.stdout.on('data', (data) => {
    process.stdout.write(data);
  });

  // Forward stderr
  serverProcess.stderr.on('data', (data) => {
    process.stderr.write(data);
  });

  // Handle server process errors
  serverProcess.on('error', (error) => {
    console.error(`Failed to start server: ${error.message}`);
    process.exit(1);
  });

  // Handle server process exit
  serverProcess.on('exit', (code) => {
    if (code !== null && code !== 0) {
      console.error(`Server exited with code ${code}`);
    }
    process.exit(code || 0);
  });

  // Auto-open browser after 1 second delay
  setTimeout(() => {
    console.log(`Opening browser at ${URL}`);
    openBrowser();
  }, 1000);

  // Handle SIGINT (Ctrl+C) to gracefully shutdown
  process.on('SIGINT', () => {
    console.log('\nShutting down server...');
    serverProcess.kill('SIGINT');
  });

  // Handle SIGTERM
  process.on('SIGTERM', () => {
    console.log('\nShutting down server...');
    serverProcess.kill('SIGTERM');
  });
}

main();

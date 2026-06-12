#!/usr/bin/env node
// Thin shim: finds rift.exe and forwards all args to it.
'use strict';

const { spawnSync } = require('child_process');
const path = require('path');
const os = require('os');
const fs = require('fs');

const EXE_NAME = 'rift.exe';

function findRiftExe() {
  // First: check next to this script (for dev / local installs)
  const localPath = path.join(__dirname, '..', EXE_NAME);
  if (fs.existsSync(localPath)) return localPath;

  // Second: check %LOCALAPPDATA%\rift\bin\rift.exe (postinstall destination)
  const appLocal = process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local');
  const installedPath = path.join(appLocal, 'rift', 'bin', EXE_NAME);
  if (fs.existsSync(installedPath)) return installedPath;

  console.error(
    '\n\x1b[31m[rift]\x1b[0m Cannot find rift.exe.\n' +
    'Try reinstalling: \x1b[36mnpm install -g rift\x1b[0m\n'
  );
  process.exit(1);
}

const exe = findRiftExe();
const result = spawnSync(exe, process.argv.slice(2), {
  stdio: 'inherit',
  windowsHide: false,
});

process.exit(result.status ?? 1);

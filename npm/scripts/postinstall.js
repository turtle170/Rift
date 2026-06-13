#!/usr/bin/env node
// Postinstall: downloads rift.exe from GitHub Releases into %LOCALAPPDATA%\rift\bin\
'use strict';

const https = require('https');
const http = require('http');
const fs = require('fs');
const path = require('path');
const os = require('os');
const crypto = require('crypto');
const { execSync } = require('child_process');

const REPO = 'turtle170/Rift';
const EXE_ASSET = 'rift.exe';
const SHA_ASSET = 'rift.exe.sha256';

function log(msg) { process.stdout.write(`\x1b[36m[rift]\x1b[0m ${msg}\n`); }
function warn(msg) { process.stdout.write(`\x1b[33m[rift]\x1b[0m ${msg}\n`); }
function err(msg) { process.stderr.write(`\x1b[31m[rift]\x1b[0m ${msg}\n`); }

function getDestDir() {
  const appLocal = process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local');
  return path.join(appLocal, 'rift', 'bin');
}

async function fetchJson(url) {
  return new Promise((resolve, reject) => {
    const opts = new URL(url);
    const req = https.request({
      hostname: opts.hostname,
      path: opts.pathname + opts.search,
      headers: {
        'User-Agent': 'rift-npm-installer/0.1.0',
        'Accept': 'application/vnd.github+json',
      },
    }, res => {
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP status ${res.statusCode}`));
      }
      let data = '';
      res.on('data', c => data += c);
      res.on('end', () => {
        try { resolve(JSON.parse(data)); }
        catch (e) { reject(new Error(`JSON parse error: ${e.message}`)); }
      });
    });
    req.on('error', reject);
    req.end();
  });
}

async function downloadFile(url, destPath) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(destPath);
    function doGet(u) {
      const proto = u.startsWith('https') ? https : http;
      proto.get(u, { headers: { 'User-Agent': 'rift-npm-installer/0.1.0' } }, res => {
        if (res.statusCode === 301 || res.statusCode === 302) {
          return doGet(res.headers.location);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} from ${u}`));
        }
        res.pipe(file);
        file.on('finish', () => file.close(resolve));
      }).on('error', reject);
    }
    doGet(url);
  });
}

async function main() {
  if (process.platform !== 'win32') {
    warn('Rift is Windows-only. Skipping install on ' + process.platform);
    return;
  }

  const pkg = require('../package.json');
  const version = pkg.version;
  const tag = `v${version}`;
  log(`Fetching release info for tag ${tag} from GitHub...`);
  let release;
  try {
    release = await fetchJson(`https://api.github.com/repos/${REPO}/releases/tags/${tag}`);
  } catch (e) {
    warn(`Failed to fetch release for tag ${tag}: ${e.message}. Falling back to latest release...`);
    try {
      release = await fetchJson(`https://api.github.com/repos/${REPO}/releases/latest`);
    } catch (e2) {
      err(`Failed to fetch latest release info: ${e2.message}`);
      err('You can manually download rift.exe from: https://github.com/' + REPO + '/releases/latest');
      process.exit(1);
    }
  }

  const assets = release.assets || [];
  const exeAsset = assets.find(a => a.name === EXE_ASSET);
  const shaAsset = assets.find(a => a.name === SHA_ASSET);

  if (!exeAsset) {
    err(`No ${EXE_ASSET} found in release ${release.tag_name}. Please install manually.`);
    process.exit(1);
  }

  const destDir = getDestDir();
  fs.mkdirSync(destDir, { recursive: true });
  const destExe = path.join(destDir, EXE_ASSET);

  log(`Downloading ${EXE_ASSET} v${release.tag_name}...`);
  await downloadFile(exeAsset.browser_download_url, destExe);

  // Verify SHA-256 if available
  if (shaAsset) {
    try {
      const tmpSha = path.join(os.tmpdir(), SHA_ASSET);
      await downloadFile(shaAsset.browser_download_url, tmpSha);
      const expected = fs.readFileSync(tmpSha, 'utf8').trim().split(/\s+/)[0].toLowerCase();
      const actual = crypto.createHash('sha256').update(fs.readFileSync(destExe)).digest('hex');
      if (expected !== actual) {
        err(`SHA-256 mismatch! Expected ${expected}, got ${actual}. Deleting download.`);
        fs.unlinkSync(destExe);
        process.exit(1);
      }
      log('SHA-256 verified ✓');
      fs.unlinkSync(tmpSha);
    } catch (e) {
      warn(`Could not verify checksum: ${e.message}`);
    }
  }

  log(`Installed to: ${destExe}`);
  log('Run \x1b[32mrift hatch\x1b[0m to meet your companion!');
}

main().catch(e => {
  err(`Unexpected error: ${e.message}`);
  process.exit(1);
});

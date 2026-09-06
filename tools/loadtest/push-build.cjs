// The release push, against a stub of circinus.sh that answers the way PUSHABUILD.md says it
// does. No browser and no network: a temporary folder of fake bundles, a local server that
// records what arrives, and the real tools/push-build.mjs driven at it.
//
//   node tools/loadtest/push-build.cjs
//
// What it is for: the push runs once per release, from CI, unattended, and the ways it can go
// wrong are quiet ones — an unsigned bundle that uploads happily and is never offered, a
// truncated transfer stored as if whole, a tag that does not match the build it publishes.
const { createHash } = require('node:crypto');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');

let failures = 0;
const ok = (label, cond, detail = '') => {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
};

const root = path.resolve(__dirname, '..', '..');
const VERSION = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
const KEY = 'cmk_test_0123456789';

/** A folder of bundles shaped the way Tauri writes them, with the signatures asked for. */
function bundles(dir, { sign = ['exe', 'tar', 'appimage'], platforms = ['win', 'mac', 'linux'] } = {}) {
  const put = (rel, body, sig) => {
    const p = path.join(dir, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, body);
    if (sig) fs.writeFileSync(`${p}.sig`, `dW50cnVzdGVkIGNvbW1lbnQ6ICR7cmVsfQpSVVFG${rel.length}\n`);
    return p;
  };
  const made = {};
  if (platforms.includes('win')) made.exe = put(`nsis/Circinus Mod Manager_${VERSION}_x64-setup.exe`, Buffer.alloc(2048, 1), sign.includes('exe'));
  if (platforms.includes('win')) made.msi = put(`msi/Circinus Mod Manager_${VERSION}_x64_en-US.msi`, Buffer.alloc(64, 9), true);
  if (platforms.includes('mac')) made.dmg = put(`dmg/Circinus Mod Manager_${VERSION}_aarch64.dmg`, Buffer.alloc(3072, 2), false);
  if (platforms.includes('mac')) made.tar = put(`macos/Circinus Mod Manager.app.tar.gz`, Buffer.alloc(1024, 3), sign.includes('tar'));
  if (platforms.includes('linux')) made.appimage = put(`appimage/Circinus Mod Manager_${VERSION}_amd64.AppImage`, Buffer.alloc(4096, 4), sign.includes('appimage'));
  if (platforms.includes('linux')) made.deb = put(`deb/Circinus Mod Manager_${VERSION}_amd64.deb`, Buffer.alloc(64, 8), true);
  return made;
}

/** circinus.sh as PUSHABUILD.md describes it, plus a record of everything that arrived. */
function stub(opts = {}) {
  const seen = { calls: [], uploads: [], published: null, notes: null };
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks);
      const send = (code, obj) => { res.writeHead(code, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(obj)); };
      const url = req.url.split('?')[0];
      if (url !== '/api/v1/releases/latest.json' && req.headers.authorization !== `Bearer ${KEY}`) return send(401, { error: 'no key' });
      seen.calls.push(`${req.method} ${url}`);

      let m = url.match(/^\/api\/v1\/ci\/releases\/([^/]+)$/);
      if (m && req.method === 'POST') {
        seen.notes = JSON.parse(body.toString() || '{}').notes ?? '';
        return send(200, { version: m[1], created: true, active: false });
      }
      m = url.match(/^\/api\/v1\/ci\/releases\/([^/]+)\/(windows|mac|linux)\/(installer|update)$/);
      if (m && req.method === 'PUT') {
        const claimed = req.headers['x-circinus-sha256'];
        const real = createHash('sha256').update(body).digest('hex');
        if (claimed && claimed !== real) return send(422, { error: 'the bytes do not match the hash you sent' });
        seen.uploads.push({
          slot: `${m[2]}/${m[3]}`,
          name: req.headers['x-circinus-filename'],
          sha256: claimed ?? null,
          signed: !!req.headers['x-circinus-signature'],
          bytes: body.length,
          type: req.headers['content-type']
        });
        return send(200, opts.warnOn === `${m[2]}/${m[3]}` ? { warning: 'this replaced an earlier file, which had already been downloaded 5 times' } : {});
      }
      m = url.match(/^\/api\/v1\/ci\/releases\/([^/]+)\/publish$/);
      if (m && req.method === 'POST') {
        const opt = JSON.parse(body.toString() || '{}');
        const platforms = [...new Set(seen.uploads.map((u) => u.slot.split('/')[0]))];
        if (platforms.length < 3 && !opt.force) return send(422, { error: 'not published: the Linux AppImage has not been uploaded. All three platforms are required.' });
        seen.published = { version: m[1], force: !!opt.force };
        const offers = seen.uploads.filter((u) => u.signed).map((u) => (u.slot.startsWith('windows') ? 'windows-x86_64' : u.slot === 'mac/update' ? 'darwin-aarch64' : 'linux-x86_64'));
        return send(200, { version: m[1], active: true, preRelease: false, updaterOffers: opts.noOffers ? [] : [...new Set(offers)], latest: m[1] });
      }
      if (url === '/api/v1/releases/latest.json') {
        if (!seen.published) return send(404, { error: 'nothing published' });
        const named = opts.staleLatest ? '0.0.1' : seen.published.version;
        return send(200, {
          version: named,
          pub_date: '2026-09-06T10:00:00Z',
          platforms: Object.fromEntries(seen.uploads.filter((u) => u.signed).map((u) => [
            u.slot.startsWith('windows') ? 'windows-x86_64' : u.slot === 'mac/update' ? 'darwin-aarch64' : 'linux-x86_64',
            { signature: opts.unsignedLatest ? '' : 'dW50cnVzdGVk', url: `https://circinus.sh/d/${encodeURIComponent(u.name)}` }
          ]))
        });
      }
      send(404, { error: 'no such route' });
    });
  });
  return { server, seen };
}

/** Run the real script against the stub and hand back what it printed and what arrived. The
 *  stub answers on this process's event loop, so the child has to be waited for, not blocked on. */
async function run(dir, opts = {}, args = []) {
  const { server, seen } = stub(opts);
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const port = server.address().port;
  const child = spawn(process.execPath, [path.join(root, 'tools', 'push-build.mjs'), '--dir', dir, ...args], {
    cwd: root,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, CIRCINUS_BASE: `http://127.0.0.1:${port}`, CIRCINUS_BUILD_KEY: KEY }
  });
  let out = '';
  child.stdout.on('data', (d) => (out += d));
  child.stderr.on('data', (d) => (out += d));
  const code = await new Promise((r) => child.on('close', r));
  await new Promise((r) => server.close(r));
  return { out, code, seen };
}

async function main() {
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'circinus-push-'));
const make = (name, opts) => { const d = path.join(tmp, name); fs.mkdirSync(d); return { dir: d, files: bundles(d, opts) }; };

// ---- a whole release ----
{
  const { dir, files } = make('full');
  const { out, code, seen } = await run(dir);
  ok('a full release exits clean', code === 0, out.split('\n').filter(Boolean).slice(-1)[0]);
  ok('it creates, uploads four files, publishes, then reads latest.json back', seen.calls.join(' | ') ===
    [`POST /api/v1/ci/releases/${VERSION}`,
     `PUT /api/v1/ci/releases/${VERSION}/windows/installer`,
     `PUT /api/v1/ci/releases/${VERSION}/mac/installer`,
     `PUT /api/v1/ci/releases/${VERSION}/mac/update`,
     `PUT /api/v1/ci/releases/${VERSION}/linux/installer`,
     `POST /api/v1/ci/releases/${VERSION}/publish`,
     `GET /api/v1/releases/latest.json`].join(' | '), seen.calls.join(' | '));
  ok('it sends the whole file as the body, not a form', seen.uploads.every((u) => u.type === 'application/octet-stream'));
  ok('every upload carries the hash of the bytes that arrived', seen.uploads.every((u) => u.sha256 && /^[0-9a-f]{64}$/.test(u.sha256)));
  const sizes = { 'windows/installer': fs.statSync(files.exe).size, 'mac/installer': fs.statSync(files.dmg).size, 'mac/update': fs.statSync(files.tar).size, 'linux/installer': fs.statSync(files.appimage).size };
  ok('and the bytes are the file, whole', seen.uploads.every((u) => u.bytes === sizes[u.slot]), JSON.stringify(seen.uploads.map((u) => `${u.slot}:${u.bytes}`)));
  ok('the NSIS installer goes up, not the .msi beside it', seen.uploads.find((u) => u.slot === 'windows/installer')?.name.endsWith('-setup.exe'));
  ok('the Mac update is the app tarball, not the disk image', seen.uploads.find((u) => u.slot === 'mac/update')?.name.endsWith('.app.tar.gz'));
  ok('everything the updater installs is signed', seen.uploads.filter((u) => u.slot !== 'mac/installer').every((u) => u.signed));
  ok('the disk image needs no signature, and is not given a false one', seen.uploads.find((u) => u.slot === 'mac/installer')?.signed === false);
  ok('it publishes without forcing when all three platforms are there', seen.published && seen.published.force === false);
  ok('and says what the updater will offer', /updater will offer/.test(out), out.split('\n').find((l) => /offer/.test(l))?.trim());
}

// ---- the quiet failures ----
{
  const { dir } = make('unsigned', { sign: ['tar', 'appimage'] });
  const { out, code, seen } = await run(dir);
  ok('an unsigned installer stops the push before anything is sent', code !== 0 && seen.calls.length === 0, `exit ${code}`);
  ok('and the message says what it costs, not just what is missing', /never offered/.test(out), out.trim().split('\n')[1]?.slice(0, 80));
}
{
  const { dir } = make('mismatch');
  const { out, code, seen } = await run(dir, {}, ['--version', '9.9.9']);
  ok('a version that is not the one built is refused', code !== 0 && seen.calls.length === 0, `exit ${code}`);
  ok('and the message names both numbers', out.includes('9.9.9') && out.includes(VERSION));
}
{
  const { dir } = make('empty-dir', { platforms: [] });
  const { code, out } = await run(dir);
  ok('an empty folder is refused rather than published as nothing', code !== 0, out.trim().split('\n').pop());
}
{
  const { dir } = make('offerless');
  const { out, code, seen } = await run(dir, { noOffers: true });
  ok('a release the updater is offered nothing from fails loudly', code !== 0 && !!seen.published, out.trim().split('\n').pop());
}
{
  const { dir } = make('stale');
  const { out, code } = await run(dir, { staleLatest: true });
  ok('latest.json still naming an older version fails the run', code !== 0 && /would not be offered/.test(out), out.trim().split('\n').pop());
}
{
  const { dir } = make('unsigned-latest');
  const { out, code } = await run(dir, { unsignedLatest: true });
  ok('latest.json missing a signature fails the run', code !== 0 && /refuse the update/.test(out), out.trim().split('\n').pop());
}

// ---- one platform, and a replaced file ----
{
  const { dir } = make('win-only', { platforms: ['win'] });
  const { out, code, seen } = await run(dir, { warnOn: 'windows/installer' });
  ok('a single-platform release publishes, saying it is going without the others', code === 0 && seen.published?.force === true, `exit ${code}`);
  ok('and names what was missing', /Mac disk image/.test(out) && /Linux AppImage/.test(out));
  ok("the site's warning about replacing a downloaded file is repeated, not swallowed", /already been downloaded/.test(out));
}

// ---- notes ----
{
  const { dir } = make('notes');
  const { seen } = await run(dir, {}, ['--notes', 'Faster scan — and a fix for “linked” folders.']);
  ok('em dashes and curly quotes are flattened, since the site rejects them', seen.notes === 'Faster scan - and a fix for "linked" folders.', JSON.stringify(seen.notes));
}
{
  const { dir } = make('dry');
  const { out, code, seen } = await run(dir, {}, ['--dry-run']);
  ok('a dry run sends nothing', code === 0 && seen.calls.length === 0);
  ok('and says what it would have sent', /would POST/.test(out) && /would PUT/.test(out));
}

fs.rmSync(tmp, { recursive: true, force: true });
}

main().then(() => {
  console.log(failures ? `\n${failures} failed` : '\nall good');
  process.exit(failures ? 1 : 0);
});

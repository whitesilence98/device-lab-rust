const { spawnSync } = require('node:child_process');
const { existsSync } = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const home = process.env.USERPROFILE || process.env.HOME || '';
const localCargo = path.join(home, '.cargo', 'bin', 'cargo.exe');
const cargo = process.platform === 'win32' && existsSync(localCargo) ? localCargo : 'cargo';
const env = {
  ...process.env,
  CARGO_HOME: path.join(root, '.cargo-home'),
  CARGO_TARGET_DIR: path.join(root, 'target'),
};
if (!existsSync(path.join(root, 'Cargo.lock'))) {
  const lock = spawnSync(cargo, ['generate-lockfile'], { cwd: root, env, stdio: 'inherit', shell: false });
  if (lock.error) console.error(lock.error.message);
  if (lock.status !== 0) process.exit(lock.status || 1);
}
const build = spawnSync(cargo, ['build', '--locked'], { cwd: root, env, stdio: 'inherit', shell: false });
if (build.error) console.error(build.error.message);
if (build.status !== 0) {
  console.error('Backend build failed. Use Visual Studio Developer PowerShell with C++ x64 Build Tools and Windows SDK.');
  process.exit(build.status || 1);
}

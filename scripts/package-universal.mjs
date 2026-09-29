import { cp, mkdir, rm, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const [version, inputDir = "dist/binaries", outDir = "dist/npm"] = process.argv.slice(2);
if (!version) throw new Error("usage: node scripts/package-universal.mjs <version> [inputDir] [outDir]");

const root = resolve(import.meta.dirname, "..");
const packageDir = resolve(root, outDir, "tlegado");
await rm(packageDir, { recursive: true, force: true });
await mkdir(join(packageDir, "bin"), { recursive: true });

const targets = ["linux-x64", "darwin-x64", "darwin-arm64", "win32-x64"];
for (const target of targets) {
  const source = join(root, inputDir, target, target.startsWith("win32") ? "tlegado.exe" : "tlegado");
  const destination = join(packageDir, "bin", target, target.startsWith("win32") ? "tlegado.exe" : "tlegado");
  await mkdir(join(packageDir, "bin", target), { recursive: true });
  await cp(source, destination);
}

const packageJson = {
  name: "tlegado",
  version,
  description: "Tlegado terminal reader",
  bin: { tlegado: "bin/tlegado.js" },
  files: ["bin", "README.md"],
  license: "MIT",
  repository: { type: "git", url: "https://github.com/WEP-56/Tlegado.git" },
};
await writeFile(join(packageDir, "package.json"), `${JSON.stringify(packageJson, null, 2)}\n`);
await writeFile(join(packageDir, "README.md"), `# tlegado\n\nTlegado ${version} terminal reader. The package contains native binaries for Linux x64, macOS Intel, macOS arm64, and Windows x64.\n`);
await writeFile(join(packageDir, "bin", "tlegado.js"), `#!/usr/bin/env node
import { arch, platform } from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
const key = platform === "win32" ? "win32-x64" : platform === "darwin" ? (arch === "arm64" ? "darwin-arm64" : "darwin-x64") : "linux-x64";
const executable = fileURLToPath(new URL(\`./\${key}/tlegado\${platform === "win32" ? ".exe" : ""}\`, import.meta.url));
const result = spawnSync(executable, process.argv.slice(2), { stdio: "inherit", windowsHide: false });
if (result.error) { console.error(result.error.message); process.exit(1); }
process.exit(result.status ?? 1);
`);
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const output = execFileSync(npm, ["pack", "--json", "--pack-destination", resolve(root, outDir)], { cwd: packageDir, encoding: "utf8", shell: process.platform === "win32" });
const packed = JSON.parse(output)[0]?.filename;
if (!packed) throw new Error("npm pack did not return a filename");
console.log(resolve(root, outDir, basename(packed)));

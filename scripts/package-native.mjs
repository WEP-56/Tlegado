import { cp, mkdir, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { execFileSync } from "node:child_process";

const [target, platform, arch, version, outDir = "dist/npm"] = process.argv.slice(2);
if (![target, platform, arch, version].every(Boolean)) {
  throw new Error("usage: node scripts/package-native.mjs <target> <platform> <arch> <version> [outDir]");
}

const root = resolve(import.meta.dirname, "..");
const binaryName = process.platform === "win32" && target.includes("windows") ? "tlegado.exe" : "tlegado";
const targetSource = join(root, "target", target, "release", binaryName);
const source = existsSync(targetSource)
  ? targetSource
  : join(root, "target", "release", process.platform === "win32" ? "tlegado.exe" : "tlegado");
const packageName = `tlegado-${platform}-${arch}`;
const packageDir = resolve(root, outDir, packageName);
await rm(packageDir, { recursive: true, force: true });
await mkdir(join(packageDir, "bin"), { recursive: true });
await cp(source, join(packageDir, "bin", binaryName));

const packageJson = {
  name: packageName,
  version,
  description: `Tlegado terminal reader binary for ${platform} ${arch}`,
  bin: { tlegado: `bin/${binaryName}` },
  os: platform === "win32" ? ["win32"] : [platform],
  cpu: arch === "x64" ? ["x64"] : [arch],
  files: ["bin", "README.md"],
  license: "MIT",
  repository: { type: "git", url: "https://github.com/WEP-56/Tlegado.git" },
};
await writeFile(join(packageDir, "package.json"), `${JSON.stringify(packageJson, null, 2)}\n`);
await writeFile(join(packageDir, "README.md"), `# ${packageName}\n\nPlatform package for Tlegado ${version}.\n\nAfter publishing or installing this package, run 'tlegado --help'.\n`);
const npmPack = execFileSync(process.platform === "win32" ? "npm.cmd" : "npm", ["pack", "--json", "--pack-destination", resolve(root, outDir)], { cwd: packageDir, encoding: "utf8", shell: process.platform === "win32" });
const packed = JSON.parse(npmPack)[0]?.filename;
if (!packed) throw new Error("npm pack did not return a filename");
console.log(resolve(root, outDir, basename(packed)));

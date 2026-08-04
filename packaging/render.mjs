#!/usr/bin/env node
/**
 * Fills the package-manager templates in this directory from a real release.
 *
 * WHY A SCRIPT AND NOT `sed`. Three ecosystems, four assets and eight
 * placeholders is exactly the amount of substitution somebody gets wrong by
 * hand at 2am on release day, and the failure mode is silent: a manifest with
 * last version's checksum installs the wrong thing, and a manifest with a
 * placeholder left in it fails review after a two-day queue.
 *
 * SO THIS FAILS LOUDLY, THREE WAYS:
 *   - an expected asset missing from --assets is an error, not a warning;
 *   - a placeholder in a template that this script does not know how to fill
 *     is an error (a template grew a field and nobody told the renderer);
 *   - rendering without --assets writes `TODO-…` checksums and says so on
 *     stderr, so output that cannot be submitted cannot be mistaken for output
 *     that can.
 *
 * No dependencies: Node's own crypto and fs. The release workflow already has
 * Node for the frontend build, and a packaging script that needs `npm install`
 * is a packaging script that breaks when the registry does.
 *
 *   node packaging/render.mjs --version 0.1.0 [--assets ./dir] [--out ./dir]
 *
 * Asset names come from Tauri's bundler; the table in README.md is the
 * source of truth for them and this file must agree with it.
 */

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

/** Template files, relative to this directory. Output mirrors the layout. */
const TEMPLATES = [
  "winget/SahilHirani.Kavka.yaml",
  "winget/SahilHirani.Kavka.installer.yaml",
  "winget/SahilHirani.Kavka.locale.en-US.yaml",
  "chocolatey/kavka.nuspec",
  "chocolatey/tools/chocolateyinstall.ps1",
  "homebrew/kavka.rb",
];

/**
 * The four release assets, and which placeholder each one's digest fills.
 *
 * `name` is a function of the version because Tauri puts the version in the
 * middle of every filename. Get one of these wrong and the manifest points at
 * a 404 that only shows up in somebody else's CI.
 */
const ASSETS = [
  { key: "SHA256_EXE", name: (v) => `Kavka_${v}_x64-setup.exe` },
  { key: "SHA256_MSI", name: (v) => `Kavka_${v}_x64_en-US.msi` },
  { key: "SHA256_DMG_ARM64", name: (v) => `Kavka_${v}_aarch64.dmg` },
  { key: "SHA256_DMG_X64", name: (v) => `Kavka_${v}_x64.dmg` },
];

function fail(message) {
  console.error(`packaging/render.mjs: ${message}`);
  process.exit(1);
}

function arg(name) {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? null : (process.argv[at + 1] ?? null);
}

const version = arg("version");
if (version === null) {
  fail("--version is required, e.g. --version 0.1.0");
}
// Not a full semver parse — just enough that `--version v0.1.0` (the tag, with
// its `v`) fails here rather than producing URLs with `vv0.1.0` in them.
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  fail(`--version must look like 1.2.3, got "${version}" (drop a leading "v")`);
}

const assetsDir = arg("assets");
const outDir = resolve(arg("out") ?? join(HERE, "out"));

/** SHA-256 of one asset, or a loud placeholder when there is nothing to hash. */
function digests() {
  const values = {};
  if (assetsDir === null) {
    console.error(
      "packaging/render.mjs: no --assets given, so checksums are placeholders. " +
        "The output is readable and NOT submittable.",
    );
    for (const asset of ASSETS) values[asset.key] = `TODO-${asset.key}`;
    return values;
  }
  const dir = resolve(assetsDir);
  const present = new Set(readdirSync(dir));
  const missing = ASSETS.filter((a) => !present.has(a.name(version)));
  if (missing.length > 0) {
    fail(
      `these release assets are not in ${dir}:\n  ` +
        missing.map((a) => a.name(version)).join("\n  ") +
        "\nCheck the artifact-name table in packaging/README.md against what " +
        "tauri-action actually uploaded.",
    );
  }
  for (const asset of ASSETS) {
    const bytes = readFileSync(join(dir, asset.name(version)));
    // Upper case: winget's own manifests use it and its validator is
    // case-insensitive, Chocolatey and Homebrew do not care.
    values[asset.key] = createHash("sha256").update(bytes).digest("hex").toUpperCase();
  }
  return values;
}

const substitutions = {
  VERSION: version,
  // The date the manifests were rendered, in UTC. winget wants ISO-8601, and
  // this is the release's date by construction: the workflow renders on the
  // same run that published it.
  RELEASE_DATE: new Date().toISOString().slice(0, 10),
  ...digests(),
};

let wrote = 0;
for (const template of TEMPLATES) {
  const source = readFileSync(join(HERE, template), "utf8");
  const unknown = new Set();
  const rendered = source.replace(/\{\{([A-Z0-9_]+)\}\}/g, (whole, key) => {
    if (!(key in substitutions)) {
      unknown.add(key);
      return whole;
    }
    return substitutions[key];
  });
  if (unknown.size > 0) {
    fail(
      `${template} uses placeholders this script cannot fill: ` +
        `${[...unknown].join(", ")}. Add them to \`substitutions\`.`,
    );
  }
  const target = join(outDir, template);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, rendered);
  wrote += 1;
}

console.log(
  `packaging/render.mjs: wrote ${wrote} manifests for ${version} to ` +
    `${relative(process.cwd(), outDir) || outDir}`,
);

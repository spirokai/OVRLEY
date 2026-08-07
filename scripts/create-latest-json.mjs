import { deepStrictEqual } from "node:assert";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";

const SUPPORTED_OPTIONS = new Set([
  "owner",
  "repo",
  "tag",
  "version",
  "notes-file",
  "staging-dir",
  "output",
  "validate-manifest",
]);
const options = parseOptions(process.argv.slice(2));
const isValidation = Boolean(options["validate-manifest"]);

for (const name of [
  "owner",
  "repo",
  "tag",
  "version",
  "notes-file",
  "staging-dir",
]) {
  requireOption(options, name);
}

if (!isValidation) {
  requireOption(options, "output");
} else if (options.output) {
  fail("--output cannot be combined with --validate-manifest");
}

assertStableVersion(options.version);
if (options.tag !== options.version) {
  fail(
    `Release tag ${JSON.stringify(options.tag)} must exactly equal version ${options.version}`,
  );
}
if (!options.owner || options.owner.includes("/") || /\s/.test(options.owner)) {
  fail("Repository owner must be a single non-empty GitHub path component");
}
if (!options.repo || options.repo.includes("/") || /\s/.test(options.repo)) {
  fail("Repository name must be a single non-empty GitHub path component");
}

const notes = await readFile(resolve(options["notes-file"]), "utf8");
const manifest = await buildManifest({
  owner: options.owner,
  repo: options.repo,
  tag: options.tag,
  version: options.version,
  notes,
  stagingDir: options["staging-dir"],
});

if (isValidation) {
  await validateManifestFile(resolve(options["validate-manifest"]), manifest);
  console.log(`[latest-json] Validated ${options["validate-manifest"]}`);
} else {
  const outputPath = resolve(options.output);
  await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
  await validateManifestFile(outputPath, manifest);
  console.log(`[latest-json] Wrote ${options.output}`);
}

async function buildManifest({
  owner,
  repo,
  tag,
  version,
  notes: releaseNotes,
  stagingDir,
}) {
  const names = canonicalNames(version);
  const files = await readStagingFiles(stagingDir, names);
  const signatures = {};

  for (const [platform, signatureName] of Object.entries(names.signatures)) {
    const signature = files[signatureName];
    if (!signature.trim()) {
      fail(`Signature ${signatureName} is empty`);
    }
    if (
      signature.includes("://") ||
      Object.values(names.public).some((name) => signature.includes(name))
    ) {
      fail(
        `Signature ${signatureName} contains a URL or filename instead of signature contents`,
      );
    }
    signatures[platform] = signature;
  }

  const platformNames = ["windows-x86_64", "darwin-aarch64", "linux-x86_64"];
  const platforms = {
    "windows-x86_64": {
      url: releaseAssetUrl(owner, repo, tag, names.public.windows),
      signature: signatures.windows,
    },
    "darwin-aarch64": {
      url: releaseAssetUrl(owner, repo, tag, names.public.macosUpdater),
      signature: signatures.macos,
    },
    "linux-x86_64": {
      url: releaseAssetUrl(owner, repo, tag, names.public.linux),
      signature: signatures.linux,
    },
  };

  if (Object.keys(platforms).join("|") !== platformNames.join("|")) {
    fail("Manifest platform keys are not canonical");
  }

  return {
    version,
    notes: releaseNotes,
    platforms,
  };
}

async function readStagingFiles(stagingDir, names) {
  const path = resolve(stagingDir);
  let entries;
  try {
    entries = await readdir(path, { withFileTypes: true });
  } catch (error) {
    fail(`Could not read staging directory ${path}: ${error.message}`);
  }

  const expected = [
    ...Object.values(names.public),
    ...Object.values(names.signatures),
  ];
  const actual = entries.map((entry) => {
    if (!entry.isFile()) {
      fail(`Staging directory contains non-file entry ${entry.name}`);
    }
    return entry.name;
  });

  if (new Set(actual).size !== actual.length) {
    fail("Staging directory contains duplicate artifact names");
  }
  if (
    actual.length !== expected.length ||
    actual.some((name) => !expected.includes(name)) ||
    expected.some((name) => !actual.includes(name))
  ) {
    fail(`Staging directory must contain exactly: ${expected.join(", ")}`);
  }

  const files = {};
  for (const name of expected) {
    files[name] = await readFile(resolve(path, name), "utf8");
    if (name.endsWith(".sig") && !files[name].trim()) {
      fail(`Signature ${name} is empty`);
    }
  }
  return files;
}

async function validateManifestFile(path, expected) {
  let source;
  let actual;
  try {
    source = await readFile(path, "utf8");
    actual = JSON.parse(source);
  } catch (error) {
    fail(`Could not read or parse manifest ${path}: ${error.message}`);
  }

  try {
    deepStrictEqual(actual, expected);
  } catch (error) {
    fail(
      `Manifest does not match the canonical generated value: ${error.message}`,
    );
  }

  if (source !== `${JSON.stringify(expected, null, 2)}\n`) {
    fail(`Manifest ${basename(path)} is not deterministically formatted`);
  }
}

function canonicalNames(version) {
  return {
    public: {
      windows: `OVRLEY-windows-x86_64-${version}-setup.exe`,
      windowsPortable: `OVRLEY-windows-x86_64-${version}-portable.zip`,
      macosDmg: `OVRLEY-macos-aarch64-${version}.dmg`,
      macosUpdater: `OVRLEY-macos-aarch64-${version}.app.tar.gz`,
      linux: `OVRLEY-linux-x86_64-${version}.AppImage`,
    },
    signatures: {
      windows: `OVRLEY-windows-x86_64-${version}-setup.exe.sig`,
      macos: `OVRLEY-macos-aarch64-${version}.app.tar.gz.sig`,
      linux: `OVRLEY-linux-x86_64-${version}.AppImage.sig`,
    },
  };
}

function releaseAssetUrl(owner, repo, tag, filename) {
  return `https://github.com/${owner}/${repo}/releases/download/${tag}/${filename}`;
}

function parseOptions(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (!argument.startsWith("--")) {
      fail(`Unexpected argument ${argument}`);
    }
    const name = argument.slice(2);
    if (!name || options[name] !== undefined) {
      fail(`Duplicate or malformed option ${argument}`);
    }
    if (!SUPPORTED_OPTIONS.has(name)) {
      fail(`Unsupported option --${name}`);
    }
    const value = args[index + 1];
    if (!value || value.startsWith("--")) {
      fail(`Option ${argument} requires a value`);
    }
    options[name] = value;
    index += 1;
  }
  return options;
}

function requireOption(options, name) {
  if (!options[name]) {
    fail(`Missing required option --${name}`);
  }
}

function assertStableVersion(value) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value)) {
    fail(
      `Release version must be stable X.Y.Z SemVer: ${JSON.stringify(value)}`,
    );
  }
}

function fail(message) {
  console.error(`[latest-json] ${message}`);
  process.exit(1);
}

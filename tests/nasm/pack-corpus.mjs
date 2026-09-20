#!/usr/bin/env node
// Bottlify infrastructure (not an upstream candidate).
//
// Packs the assembled nasm test corpus -- every `build/<name>.img` with its
// gdb-generated `build/<name>.fixture` -- into one gzipped JSON document, so a
// consumer that has neither nasm nor gdb can run the same cases against its
// own machine and compare with the same hardware-recorded state.
//
// Usage: pack-corpus.mjs <output.json.gz>
//
// The document: { commit, generated, cases: [{ name, image, fixture }] } where
// `image` is the ELF32 file base64-encoded and `fixture` is the fixture text
// exactly as gdb wrote it (the consumer parses the ---BEGIN JSON--- block the
// way run.js does). A case whose fixture is missing is an error, not a skip:
// the corpus is only useful whole.

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import zlib from "node:zlib";
import { execFileSync } from "node:child_process";

const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const BUILD_DIR = path.join(__dirname, "build");

const output = process.argv[2];
if(!output)
{
    console.error("usage: pack-corpus.mjs <output.json.gz>");
    process.exit(1);
}

const names = fs.readdirSync(BUILD_DIR)
    .filter(name => name.endsWith(".img"))
    .map(name => name.slice(0, -4))
    .sort();

if(names.length === 0)
{
    console.error("pack-corpus: no images in " + BUILD_DIR + " (run create_tests.js first)");
    process.exit(1);
}

const cases = names.map(name => {
    const fixture_path = path.join(BUILD_DIR, name + ".fixture");
    if(!fs.existsSync(fixture_path))
    {
        console.error("pack-corpus: " + name + " has no fixture (run gen_fixtures.js first)");
        process.exit(1);
    }
    return {
        name,
        image: fs.readFileSync(path.join(BUILD_DIR, name + ".img")).toString("base64"),
        fixture: fs.readFileSync(fixture_path, "latin1"),
    };
});

let commit = "unknown";
try
{
    commit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: __dirname, encoding: "utf8" }).trim();
}
catch(e)
{
    // Not a checkout: the field says so.
}

const document = {
    commit,
    generated: new Date().toISOString(),
    cases,
};

fs.writeFileSync(output, zlib.gzipSync(Buffer.from(JSON.stringify(document)), { level: 9 }));
console.log("pack-corpus: " + cases.length + " cases -> " + output);

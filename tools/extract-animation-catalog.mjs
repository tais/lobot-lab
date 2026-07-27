import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const projectRoot = resolve(import.meta.dirname, "..");
const sourceArgument = process.argv[2] || process.env.JA2_SOURCE_ROOT;
if (!sourceArgument) {
  throw new Error(
    "Pass the JA2 1.13 source directory as an argument or set JA2_SOURCE_ROOT"
  );
}
const sourceRoot = resolve(sourceArgument);
const control = readFileSync(
  resolve(sourceRoot, "Tactical/Animation Control.cpp"),
  "utf8"
);
const bodyTypeDb = readFileSync(
  resolve(sourceRoot, "Tactical/LogicalBodyTypes/BodyTypeDB.cpp"),
  "utf8"
);
const animationData = readFileSync(
  resolve(sourceRoot, "Tactical/Animation Data.cpp"),
  "utf8"
);

const stateStart = bodyTypeDb.indexOf('LOGBT_ENUMDB_ADD("AnimationStates"');
const stateEnd = bodyTypeDb.indexOf('LOGBT_ENUMDB_ADD("AnimationSurfaces"');
if (stateStart < 0 || stateEnd < 0) {
  throw new Error("Could not locate the animation state enumerator table");
}
const stateNames = [
  ...bodyTypeDb
    .slice(stateStart, stateEnd)
    .matchAll(/^\s*([A-Z][A-Z0-9_]+),?\s*$/gm)
].map((match) => match[1]);

const labelStart = control.indexOf("gAnimControl[ NUMANIMATIONSTATES ]");
const labelEnd = control.indexOf("\n};", labelStart);
const labels = [
  ...control
    .slice(labelStart, labelEnd)
    .matchAll(/^\s*\{?\s*"([^"]+)"\s*,/gm)
].map((match) => match[1].replaceAll("\t", " "));
if (stateNames.length !== labels.length) {
  throw new Error(
    `State/label count mismatch: ${stateNames.length} states, ${labels.length} labels`
  );
}

const bodies = ["REGMALE", "BIGMALE", "STOCKYMALE", "REGFEMALE"];
const mappings = Object.fromEntries(
  stateNames.map((state, index) => [
    state,
    {
      id: state,
      label: labels[index],
      base: {},
      item: {},
      waterTwoHanded: {},
      waterOther: {}
    }
  ])
);

for (const body of bodies) {
  const basePattern = new RegExp(
    `gubAnimSurfaceIndex\\[\\s*${body}\\s*\\]\\[\\s*([A-Z0-9_]+)\\s*\\]\\s*=\\s*([A-Z0-9_]+)\\s*;`,
    "g"
  );
  const itemPattern = new RegExp(
    `gubAnimSurfaceItemSubIndex\\[\\s*${body}\\s*\\]\\[\\s*([A-Z0-9_]+)\\s*\\]\\s*=\\s*([A-Z0-9_]+)\\s*;`,
    "g"
  );
  const waterPattern = new RegExp(
    `gubAnimSurfaceMidWaterSubIndex\\[\\s*${body}\\s*\\]\\[\\s*([A-Z0-9_]+)\\s*\\]\\[\\s*([01])\\s*\\]\\s*=\\s*([A-Z0-9_]+)\\s*;`,
    "g"
  );
  for (const match of control.matchAll(basePattern)) {
    mappings[match[1]].base[body] = match[2];
  }
  for (const match of control.matchAll(itemPattern)) {
    mappings[match[1]].item[body] = match[2];
  }
  for (const match of control.matchAll(waterPattern)) {
    const target =
      match[2] === "0"
        ? mappings[match[1]].waterTwoHanded
        : mappings[match[1]].waterOther;
    target[body] = match[3];
  }
}

const records = stateNames
  .map((state) => mappings[state])
  .filter((record) => Object.keys(record.base).length > 0);
const target = resolve(projectRoot, "src-tauri/animation-catalog.json");
writeFileSync(target, `${JSON.stringify(records, null, 2)}\n`);
console.log(`Wrote ${records.length} engine animation states to ${target}`);

const databaseStart = animationData.indexOf(
  "gAnimSurfaceDatabase[ NUMANIMATIONSURFACETYPES ]"
);
const databaseEnd = animationData.indexOf("\n};", databaseStart);
if (databaseStart < 0 || databaseEnd < 0) {
  throw new Error("Could not locate the physical animation surface database");
}
const physicalSurfaces = Object.fromEntries(
  [
    ...animationData
      .slice(databaseStart, databaseEnd)
      .matchAll(/^\s*([A-Z][A-Z0-9_]+),\s*"([^"]+\.STI)"/gm)
  ].map((match) => [match[1], match[2].replaceAll("\\\\", "\\")])
);
const physicalTarget = resolve(
  projectRoot,
  "src-tauri/physical-surface-catalog.json"
);
writeFileSync(physicalTarget, `${JSON.stringify(physicalSurfaces, null, 2)}\n`);
console.log(
  `Wrote ${Object.keys(physicalSurfaces).length} physical animation surfaces to ${physicalTarget}`
);

import { readFile } from "node:fs/promises";

const manifest = JSON.parse(await readFile(new URL("../extension/manifest.json", import.meta.url), "utf8"));
if (manifest.manifest_version !== 3) throw new Error("HAWK Browser Bridge must use Manifest V3.");
const permissions = new Set(manifest.permissions ?? []);
for (const permission of ["activeTab", "scripting", "storage"]) {
  if (!permissions.has(permission)) throw new Error(`Missing extension permission: ${permission}`);
}
if (manifest.host_permissions?.length) throw new Error("Host permissions must remain optional and user-granted.");
console.log("HAWK Browser Bridge extension validated.");

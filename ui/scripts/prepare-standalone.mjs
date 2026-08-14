import { copyFile, mkdir, rename, rm, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

if (process.platform !== "linux") {
  process.exit(0);
}

function desktopExecArg(value) {
  if (!path.isAbsolute(value) || /[\0\r\n]/u.test(value)) {
    throw new Error("standalone executable path must be an absolute single-line path");
  }

  let escaped = "";
  for (const character of value) {
    if ('\\"`$'.includes(character)) {
      escaped += `\\${character}`;
    } else if (character === "%") {
      escaped += "%%";
    } else {
      escaped += character;
    }
  }
  return `"${escaped}"`;
}

function desktopString(value) {
  if (/[\0\r\n]/u.test(value)) {
    throw new Error("desktop entry value must be a single line");
  }
  return value.replaceAll("\\", "\\\\");
}

async function replaceFile(source, destination) {
  const temporary = `${destination}.tmp-${process.pid}`;
  await rm(temporary, { force: true });
  try {
    if (source) {
      await copyFile(source, temporary);
    } else {
      await writeFile(temporary, desktopEntry, { encoding: "utf8", mode: 0o644 });
    }
    await rename(temporary, destination);
  } catch (error) {
    await rm(temporary, { force: true });
    throw error;
  }
}

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const uiDirectory = path.resolve(scriptDirectory, "..");
const repositoryDirectory = path.resolve(uiDirectory, "..");
const configuredDataHome = process.env.XDG_DATA_HOME?.trim();
const dataHome =
  configuredDataHome && path.isAbsolute(configuredDataHome)
    ? configuredDataHome
    : path.join(homedir(), ".local", "share");

if (!path.isAbsolute(dataHome)) {
  throw new Error("cannot resolve an absolute XDG data directory");
}

const applicationsDirectory = path.join(dataHome, "applications");
const iconsDirectory = path.join(dataHome, "icons", "hicolor", "512x512", "apps");
const desktopPath = path.join(applicationsDirectory, "m590-ui.desktop");
const iconPath = path.join(iconsDirectory, "m590-ui.png");
const iconSource = path.join(uiDirectory, "src-tauri", "icons", "icon.png");
const executable = path.join(repositoryDirectory, "target", "release", "m590-ui");
const desktopEntry = `[Desktop Entry]
Type=Application
Name=M590Bridge
Comment=Local clipboard and file bridge
Exec=${desktopExecArg(executable)}
Icon=${desktopString(iconPath)}
Terminal=false
Categories=Utility;
StartupWMClass=m590-ui
NoDisplay=true
X-M590Bridge-Managed=standalone
`;

await mkdir(applicationsDirectory, { recursive: true });
await mkdir(iconsDirectory, { recursive: true });
await replaceFile(iconSource, iconPath);
await replaceFile(null, desktopPath);
console.log(`linux_desktop_identity=${desktopPath}`);

import { enUSMessages } from "../src/i18n/messages/en-US.ts";
import fs from "node:fs";
import path from "node:path";

function walk(d, out) {
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, e.name).split(path.sep).join("/");
    if (e.isDirectory()) {
      if (p.endsWith("i18n/messages")) continue;
      walk(p, out);
    } else if (/\.(ts|vue)$/.test(e.name)) {
      out.push(p);
    }
  }
}

const files = [];
walk("src", files);
const blobs = files.map((f) => fs.readFileSync(f, "utf8"));
const keys = Object.keys(enUSMessages);
const dead = [];
for (const k of keys) {
  // JSON.stringify 给出源文件中的带引号转义形式（含 \n 等），同时兼顾模板里的单引号调用
  const dq = JSON.stringify(k);
  const sq = "'" + k + "'";
  if (!blobs.some((b) => b.includes(dq) || b.includes(sq))) dead.push(k);
}
console.log("TOTAL=" + keys.length + " DEAD=" + dead.length);
for (const k of dead) console.log("::" + k);

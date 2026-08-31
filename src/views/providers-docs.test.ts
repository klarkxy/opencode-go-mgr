import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";

const design = readFileSync(new URL("../../DESIGN.md", import.meta.url), "utf8");
const userIndexEn = readFileSync(new URL("../../docs/USER.md", import.meta.url), "utf8");
const userIndexZh = readFileSync(new URL("../../docs/USER.zh-CN.md", import.meta.url), "utf8");
const docsIndexEn = readFileSync(new URL("../../docs/README.md", import.meta.url), "utf8");
const docsIndexZh = readFileSync(new URL("../../docs/README.zh-CN.md", import.meta.url), "utf8");

const userDir = new URL("../../docs/user/", import.meta.url);
const chapterFiles = readdirSync(userDir)
  .filter((name) => name.endsWith(".md"))
  .sort();
const enChapterFiles = chapterFiles.filter((name) => !name.endsWith(".zh-CN.md"));
const zhChapterFiles = chapterFiles.filter((name) => name.endsWith(".zh-CN.md"));
const readChapter = (name: string) =>
  readFileSync(new URL(name, userDir), "utf8");
const userEn = enChapterFiles.map(readChapter).join("\n");
const userZh = zhChapterFiles.map(readChapter).join("\n");

function headings(markdown: string): string[] {
  return markdown
    .split(/\r?\n/)
    .filter((line) => /^#{1,3} /.test(line));
}

function chapterLinks(markdown: string, title: string): string[] {
  const start = markdown.indexOf(title);
  assert.ok(start >= 0, `missing ${title}`);
  const next = markdown.indexOf("\n## ", start + title.length);
  assert.ok(next > start, `chapter list for ${title} has no following H2`);
  return markdown
    .slice(start, next)
    .split(/\r?\n/)
    .filter((line) => /^\s*- \[/.test(line));
}

test("DESIGN.md names Providers as the fourth of seven views", () => {
  assert.match(
    design,
    /Dashboard, Access Keys, Accounts, Providers, Applications, Logs, Settings/,
  );
  assert.doesNotMatch(design, /Accounts, Pricing, Applications/);
  assert.match(design, /Providers is the supplier control plane/);
  assert.match(design, /binary switch bound to the effective enabled state/);
  assert.match(
    design,
    /one account-wide protocol selector and a compact mapping table of public name to upstream ID/,
  );
  assert.match(design, /locks every single or sequential batch test to that exact account with no fallback/);
  assert.match(design, /may consume quota/);
  assert.match(design, /never automatic on page load/);
  assert.match(design, /Do call the access credential “Key”/);
});

test("USER index pages keep matching chapter lists with Providers replacing Pricing", () => {
  const enChapters = chapterLinks(userIndexEn, "## Chapters");
  const zhChapters = chapterLinks(userIndexZh, "## 章节");
  assert.equal(enChapters.length, zhChapters.length);
  assert.match(userIndexEn, /- \[Providers\]\(user\/providers\.md\)/);
  assert.match(userIndexZh, /- \[供应商\]\(user\/providers\.zh-CN\.md\)/);
  assert.doesNotMatch(userIndexEn, /- \[Pricing\]\(user\//);
  assert.doesNotMatch(userIndexZh, /- \[价格表\]\(user\//);
});

test("USER chapter files pair EN/ZH with matching heading structure", () => {
  assert.deepEqual(
    enChapterFiles.map((name) => name.replace(/\.md$/, ".zh-CN.md")),
    zhChapterFiles,
  );
  for (const name of enChapterFiles) {
    const en = readChapter(name);
    const zh = readChapter(name.replace(/\.md$/, ".zh-CN.md"));
    assert.equal(
      headings(en).length,
      headings(zh).length,
      `heading count mismatch in ${name}`,
    );
  }
  assert.match(userEn, /# Providers/);
  assert.match(userZh, /# 供应商/);
});

test("USER guides describe the Providers control plane and drop stale locations", () => {
  assert.match(userEn, /Configurable HTTP adapter, not a base class/);
  assert.match(userZh, /Configurable HTTP 适配器，不是基类/);
  assert.match(userEn, /`Provider\(contract_scope_id\)`/);
  assert.match(userZh, /`Provider\(contract_scope_id\)`/);
  assert.match(userEn, /`CustomEndpoint\(account_id\)`/);
  assert.match(userZh, /`CustomEndpoint\(account_id\)`/);
  assert.match(userEn, /Chat Completions, Responses,\s+and Messages/);
  assert.match(userZh, /Chat Completions、Responses、Messages/);
  assert.match(userEn, /may\s+consume quota/);
  assert.match(userZh, /可能消耗\s*额度/);
  assert.match(userEn, /\?view=pricing/);
  assert.match(userZh, /\?view=pricing/);
  assert.match(userEn, /schema v34/i);
  assert.match(userZh, /schema v34/);
  assert.doesNotMatch(userEn, /\*\*Open\s+provider\*\*/);
  assert.doesNotMatch(userZh, /\*\*前往供应商\*\*/);
  assert.match(userEn, /GOAT cards show a clearly labelled local estimate/);
  assert.match(userZh, /GOAT 卡片显示的是明确标注的本地估算/);
  assert.match(userEn, /priced OCG[\s\S]*\$14 \/ \$35 \/ \$70/);
  assert.match(userZh, /OCG 内已定价请求日志[\s\S]*\$14 \/ \$35 \/ \$70/);
  assert.doesNotMatch(userEn, /There is no separate provider page/);
  assert.doesNotMatch(userZh, /没有独立的供应商页/);
  assert.doesNotMatch(userEn, /Use the card's \*\*Fetch models\*\* action/);
  assert.doesNotMatch(userZh, /通过卡片的 \*\*获取模型\*\* 动作刷新/);
});

test("USER guides keep unified catalog refresh and probes manual-only", () => {
  assert.match(userEn, /Command Code uses its public official `\/models` directory/);
  assert.match(userZh, /Command Code 使用官方公开的 `\/models` 目录/);
  assert.match(userEn, /no separate\s+Max or account-level GOAT\/All mode/);
  assert.match(userZh, /不再存在独立的 Max 或账号级 GOAT\/全部模式/);
  assert.doesNotMatch(userEn, /SCNet remains archived/);
  assert.doesNotMatch(userZh, /SCNet 已归档/);
  assert.match(userEn, /Every refreshable scope uses the same action/);
  assert.match(userZh, /所有可刷新的范围使用同一个动作/);
  assert.match(userEn, /static catalog is the initial\s+preset/);
  assert.match(userZh, /内置静态目录只是初始预设/);
  assert.match(userEn, /newly added by a refresh[\s\S]*MiniMax CN and Kimi Code CN[\s\S]*Chat Completions/);
  assert.match(userZh, /刷新新增的模型[\s\S]*MiniMax CN 与 Kimi Code CN[\s\S]*Chat Completions/);
  assert.match(userEn, /no refresh-account selector/);
  assert.match(userZh, /没有刷新账号选择器/);
  assert.match(userEn, /Client requests never probe/);
  assert.match(userZh, /客户端请求不会探测/);
  assert.match(userEn, /effective enabled protocol/);
  assert.match(userZh, /有效启用协议/);
});

test("docs index routes user facts through Providers and the contract module", () => {
  assert.match(docsIndexEn, /Provider contracts/);
  assert.match(docsIndexZh, /供应商合约/);
  assert.match(docsIndexEn, /provider_contracts\.rs/);
  assert.match(docsIndexZh, /provider_contracts\.rs/);
  assert.match(docsIndexEn, /ConfigurableHttpAdapter/);
  assert.match(docsIndexZh, /ConfigurableHttpAdapter/);
  assert.match(docsIndexEn, /Do not claim there is\s+no supplier page/);
  assert.match(docsIndexZh, /存在独立的供应商页/);
});

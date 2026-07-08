import { chromium } from "@playwright/test";

const BASE_URL = "http://127.0.0.1:30001";

async function main() {
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
  });
  const page = await context.newPage();

  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));

  // Start from a clean dev-mock state so the test is deterministic.
  await page.goto(BASE_URL, { waitUntil: "networkidle" });
  await page.evaluate(() => localStorage.clear());
  await page.reload({ waitUntil: "networkidle" });
  await page.waitForTimeout(800);

  // helper: click menu item by text
  async function goTo(label) {
    await page.locator(".n-menu-item").filter({ hasText: label }).click();
    await page.waitForTimeout(400);
    const current = page.locator(".n-layout-content");
    return current;
  }

  // ── 1. Dashboard ──────────────────────────────────────────
  console.log("=== 1. Dashboard ===");
  let content = await goTo("仪表盘");
  let html = await content.innerHTML();

  const statCards = page.locator(".n-statistic");
  console.log(`  stat cards: ${await statCards.count()}`);

  const descItems = page.locator(".n-descriptions-item");
  console.log(`  desc items: ${await descItems.count()}`);

  // Gateway should be reported as running by the dev mock.
  const runningTag = page.locator(".n-descriptions .n-tag").filter({ hasText: "运行中" });
  console.log(`  gateway running tag visible: ${await runningTag.isVisible()}`);

  await page.screenshot({ path: "playwright-out/01-dashboard.png" });

  // ── 2. Accounts — empty state ─────────────────────────────
  console.log("\n=== 2. Accounts (empty) ===");
  content = await goTo("账号管理");

  const emptyHint = page.locator(".n-empty");
  console.log(`  empty hint visible: ${await emptyHint.isVisible()}`);

  const addBtn = page.locator("button").filter({ hasText: "添加" }).first();
  console.log(`  "添加" button: ${await addBtn.isVisible()}`);

  await page.screenshot({ path: "playwright-out/02-accounts-empty.png" });

  // ── 3. Accounts — add and save an account ─────────────────
  console.log("\n=== 3. Accounts — add & save ===");
  await addBtn.click();
  await page.waitForTimeout(400);

  const modal = page.locator(".n-modal");
  console.log(`  modal visible: ${await modal.isVisible()}`);

  await page.locator(".n-modal input").first().fill("测试账号");
  await page.locator(".n-modal input[type='password']").fill("sk-test-key-12345");
  await page.locator(".n-modal input").nth(2).fill("REF123");
  await page.locator(".n-modal input").nth(3).fill("2026-07-01");

  const saveBtn = page.locator(".n-modal .n-button").filter({ hasText: "保存" });
  await saveBtn.click();
  await page.waitForTimeout(600);

  // After saving, the modal should close and the account card should appear.
  const accountCard = page.locator(".n-card").filter({ hasText: "测试账号" });
  const cardVisible = await accountCard.isVisible();
  console.log(`  saved account visible: ${cardVisible}`);

  const cards = page.locator(".n-card");
  console.log(`  account cards: ${await cards.count()}`);

  await page.screenshot({ path: "playwright-out/03-account-saved.png" });

  // ── 4. Dashboard reflects the new account ─────────────────
  console.log("\n=== 4. Dashboard after adding account ===");
  content = await goTo("仪表盘");
  await page.waitForTimeout(400);

  const accountOverview = page.locator(".n-list-item");
  console.log(`  overview items: ${await accountOverview.count()}`);

  const summaryText = await page.locator(".n-statistic").first().textContent();
  console.log(`  account summary: ${summaryText?.replace(/\s+/g, " ").trim()}`);

  await page.screenshot({ path: "playwright-out/04-dashboard-with-account.png" });

  // ── 5. Logs — tabs and filters ────────────────────────────
  console.log("\n=== 5. Logs ===");
  content = await goTo("日志");

  const tabs = page.locator(".n-tabs .n-tabs-tab");
  const tabCount = await tabs.count();
  console.log(`  tabs: ${tabCount}`);

  if (tabCount >= 2) {
    await tabs.nth(1).click();
    await page.waitForTimeout(300);
    console.log("  switched to 透传日志 tab");
  }

  const selects = page.locator(".n-select");
  console.log(`  filter selects: ${await selects.count()}`);

  const tableRows = page.locator(".n-data-table tr");
  console.log(`  table rows: ${await tableRows.count()}`);

  await page.screenshot({ path: "playwright-out/05-logs.png" });

  // ── 6. Settings ───────────────────────────────────────────
  console.log("\n=== 6. Settings ===");
  content = await goTo("设置");

  const inputs = page.locator("input");
  const inputCount = await inputs.count();
  console.log(`  inputs: ${inputCount}`);

  const numberInput = page.locator(".n-input-number input").first();
  if (await numberInput.isVisible()) {
    const portVal = await numberInput.inputValue();
    console.log(`  port value: ${portVal}`);
  }

  const selectTrigger = page.locator(".n-select").first();
  console.log(`  strategy select visible: ${await selectTrigger.isVisible()}`);

  const pwdInputs = page.locator("input[type='password']");
  console.log(`  password inputs: ${await pwdInputs.count()}`);

  const settingBtns = page.locator("button");
  const btnTexts = await settingBtns.allTextContents();
  console.log(`  buttons: ${btnTexts.filter(Boolean).join(", ")}`);

  await page.screenshot({ path: "playwright-out/06-settings.png" });

  // ── 7. Sidebar collapse ───────────────────────────────────
  console.log("\n=== 7. Sidebar collapse ===");
  await goTo("仪表盘");

  const trigger = page.locator(".n-layout-toggle-button");
  const triggerVisible = await trigger.isVisible();
  console.log(`  trigger visible: ${triggerVisible}`);

  if (triggerVisible) {
    await trigger.click();
    await page.waitForTimeout(500);
    await page.screenshot({ path: "playwright-out/07-collapsed.png" });

    await trigger.click();
    await page.waitForTimeout(300);
  }

  await page.screenshot({ path: "playwright-out/07-final.png" });

  // ── Summary ───────────────────────────────────────────────
  console.log("\n=== Summary ===");
  console.log(`  Total page errors: ${errors.length}`);
  if (errors.length > 0) {
    errors.forEach((e) => console.log(`    ! ${e.substring(0, 200)}`));
  }

  const ok = errors.length === 0 && cardVisible;
  if (ok) {
    console.log("\n✓ UI smoke test passed — 8 screenshots saved to playwright-out/");
  } else {
    console.log("\n✗ UI smoke test failed");
    process.exitCode = 1;
  }

  await browser.close();
}

main().catch((err) => {
  console.error("FATAL:", err.message);
  process.exit(1);
});

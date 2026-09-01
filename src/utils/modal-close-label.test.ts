import assert from "node:assert/strict";
import test from "node:test";
import { watch } from "vue";
import { setLocale } from "../i18n/index.ts";
import { applyModalCloseAriaLabel, modalCloseAriaLabel } from "./modal-close-label.ts";

type StubElement = {
  attrs: Record<string, string>;
  setAttribute(name: string, value: string): void;
};

function stubRoot(elements: StubElement[]) {
  return {
    queried: [] as string[],
    querySelectorAll(selector: string) {
      this.queried.push(selector);
      return elements;
    },
  };
}

function stubElement(attrs: Record<string, string>): StubElement {
  return {
    attrs: { ...attrs },
    setAttribute(name, value) {
      this.attrs[name] = value;
    },
  };
}

test("modal close accessible name is localized, never naive-ui's hardcoded English", () => {
  setLocale("zh-CN");
  assert.equal(modalCloseAriaLabel(), "关闭对话框");
  setLocale("en-US");
  assert.equal(modalCloseAriaLabel(), "Close dialog");
  assert.notEqual(modalCloseAriaLabel(), "close");
});

test("applyModalCloseAriaLabel rewrites naive close buttons scoped to the modal class", () => {
  const elements = [stubElement({ "aria-label": "close" }), stubElement({})];
  const root = stubRoot(elements);

  setLocale("zh-CN");
  applyModalCloseAriaLabel(root as unknown as ParentNode, "account-modal");
  assert.deepEqual(root.queried, [".account-modal .n-base-close"]);
  assert.equal(elements[0].attrs["aria-label"], "关闭对话框");
  assert.equal(elements[1].attrs["aria-label"], "关闭对话框");

  // Re-applying after a locale switch keeps the label in the active language.
  setLocale("en-US");
  applyModalCloseAriaLabel(root as unknown as ParentNode, "account-modal");
  assert.equal(elements[0].attrs["aria-label"], "Close dialog");
});

test("a stored lazy locale activates its catalog at startup without a locale change", async () => {
  // Simulate app startup with a stored fr-FR locale in a fresh i18n module
  // instance: `locale` starts at fr-FR while the catalog is still the zh-CN
  // fallback, then the lazy warmup swaps the catalog in place. The close-label
  // fix relies on `effectiveCatalog` firing here while `locale` never changes.
  const globals = globalThis as Record<string, unknown>;
  const previousWindow = globals.window;
  globals.window = {
    localStorage: { getItem: () => "fr-FR", setItem: () => {} },
  };
  try {
    const specifier = "../i18n/index.ts?lazy-startup-fr";
    const fresh = (await import(specifier)) as typeof import("../i18n/index.ts");

    assert.equal(fresh.locale.value, "fr-FR");
    // The lazy chunk has not arrived yet, so the catalog is the zh-CN fallback.
    assert.equal(fresh.t("关闭对话框"), "关闭对话框");

    let localeFires = 0;
    let catalogFires = 0;
    watch(fresh.locale, () => { localeFires += 1; });
    watch(fresh.effectiveCatalog, () => { catalogFires += 1; });

    for (let attempt = 0; attempt < 200 && fresh.t("关闭对话框") === "关闭对话框"; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 10));
    }

    assert.equal(fresh.locale.value, "fr-FR");
    assert.equal(fresh.t("关闭对话框"), "Fermer la boîte de dialogue");
    assert.equal(localeFires, 0, "lazy startup activation must not change locale");
    assert.ok(catalogFires > 0, "effectiveCatalog must fire when the lazy catalog activates");
  } finally {
    if (previousWindow === undefined) {
      delete globals.window;
    } else {
      globals.window = previousWindow;
    }
  }
});

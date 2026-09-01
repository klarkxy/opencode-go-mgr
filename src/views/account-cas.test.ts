import assert from "node:assert/strict";
import test from "node:test";
import {
  ACCOUNT_REVISION_UNAVAILABLE_MESSAGE,
  reconcileEditingAccount,
  withFreshAccountRevision,
} from "./account-cas.ts";

test("a missing fresh revision aborts the mutation before it can send a request", async () => {
  let mutationCalls = 0;
  await assert.rejects(
    withFreshAccountRevision(async () => null, async () => {
      mutationCalls += 1;
    }),
    new RegExp(ACCOUNT_REVISION_UNAVAILABLE_MESSAGE),
  );
  assert.equal(mutationCalls, 0);
});

test("reconcileEditingAccount retains only accounts present in the refreshed list", () => {
  const loaded = [{ id: "a1" }, { id: "a2" }];

  // Surviving account: fresh copy returned, caller keeps the modal open.
  assert.deepEqual(reconcileEditingAccount(loaded, "a2"), { id: "a2" });

  // Deleted account: null, caller must close the modal so it cannot morph
  // into create mode.
  assert.equal(reconcileEditingAccount(loaded, "gone"), null);
  assert.equal(reconcileEditingAccount(loaded, null), null);
  assert.equal(reconcileEditingAccount([], "a1"), null);
});

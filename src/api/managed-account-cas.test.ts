import assert from "node:assert/strict";
import test from "node:test";
import { dashboardApi } from "./dashboard.ts";
import {
  installFetchMock,
  setupControlPlane,
  v3AccountDto,
} from "../test-helpers/dashboard-v3-fetch.ts";

test("managed account writes include the current CAS tokens", async () => {
  setupControlPlane(21, 99);
  const requests = installFetchMock(() => ({
    account: v3AccountDto("managed-1", {
      name: "Managed",
      username: "note@example.com",
      accountType: "managed",
      setupStep: "google_account",
    }),
  }));

  await dashboardApi.createManagedAccount({ name: "Managed", username: "note@example.com" });
  await dashboardApi.advanceAccountSetup("managed-1", "opencode_registration");
  await dashboardApi.verifyManagedAccountKey("managed-1", "sk-secret");
  await dashboardApi.resetAccountCooldown("managed-1");
  await dashboardApi.resetAccountBrowserProfile("managed-1");

  assert.deepEqual(requests.map(({ body }) => body), [
    { name: "Managed", username: "note@example.com", expectedRevision: 21, processGeneration: 99 },
    { setupStep: "opencode_registration", expectedRevision: 21, processGeneration: 99 },
    { key: "sk-secret", expectedRevision: 21, processGeneration: 99 },
    { expectedRevision: 21, processGeneration: 99 },
    { expectedRevision: 21, processGeneration: 99 },
  ]);
});

[简体中文](external-integrations.zh-CN.md)

# External Integrations

External integrations are optional, locally supported services that extend OCG
Manager without becoming a Provider, a Plan, or a plugin. The dashboard keeps
its eight core views; supported surfaces appear in the general **Extensions**
group below the Settings divider.

## CPA

CPA (CLI Proxy API) is a local subscription runtime. OCG Manager can manage
its supported Codex, Claude, Antigravity, Kimi, and xAI account flows and route
the resulting subscription pool, but CPA remains the owner of OAuth browser
sessions, tokens, auth files, and internal scheduling. OCG stores only its
local connection configuration, the two CPA access credentials, and a local
model snapshot. The Management Key stays encrypted in OCG storage and, for a
managed child, reaches CPA only as `MANAGEMENT_PASSWORD`. CPA itself requires
client `api-keys` in its config, so the protected Inference Key and any
direct-client keys are necessarily present in CPA's local config under the OCG
data directory. Creating a client key still returns that secret once from V3;
list views stay fingerprinted.

Use one of these local deployments:

- **Installed Windows x64 desktop:** OCG can download the official Windows x64
  CPA release, keep it under the OCG data directory, and start it as an
  OCG-owned child. Start is manual; the child stops when OCG exits. OCG never
  stops a CPA process it did not start.
- **Desktop or CLI:** run CPA on the same machine and configure a loopback URL
  such as `http://127.0.0.1:8317`.
- **Docker:** enable the optional Compose sibling described in
  [Docker](docker.md). OCG uses the read-only `http://cpa:8317` service URL;
  the dashboard does not accept a LAN, Internet, or cross-node CPA address.

CPA is intentionally not a remote integration. URLs with embedded credentials,
queries, fragments, redirects, or non-loopback hosts are rejected. Do not
reuse an OCG Manager Key as either CPA key.

### Connect and operate

1. On installed Windows x64 desktop, install or start the managed CPA runtime
   from **Extensions → CPA**, or install and start CPA yourself on loopback
   and save its **Management Key** and **Inference Key**. The managed runtime
   generates those keys. Extra direct-client keys are shown fingerprinted, and
   a newly created secret is returned once. The OCG-protected Inference Key
   cannot be deleted. The Management Key is not written into CPA's
   `config.yaml`; the Inference Key and direct-client keys are, because CPA
   requires `api-keys` in that file.
2. Open **Extensions → CPA**, save the local address and both keys when you
   connect an external CPA, then run the connection test. It reports reachability, supported CPA version,
   Management authentication, and Inference authentication separately.
   OCG requires CPA 7.1.0 or newer; later major versions continue through the
   same typed response and exact-account validation instead of being rejected
   solely for their version number.
3. Start an OAuth flow from CPA's account table. Browser-callback providers
   use CPA's loopback callback ports; Kimi and xAI use their device-code flow.
   OCG never runs an OAuth callback server and does not restore an old flow
   after a refresh or restart.
4. Refresh the CPA model catalog and enable the CPA subscription pool when it
   is ready. Its single **CPA subscription pool** card on Accounts can be
   ordered and enabled/disabled like other route candidates, but cannot expose
   a Key, be deleted, or stand in for individual CPA OAuth accounts.

Disabling the pool removes it from routing without forgetting CPA setup.
**Disconnect and clear** removes OCG's CPA configuration, the pool card, and
the local model snapshot after confirmation; it does not delete CPA's OAuth
files. A CPA fault simply removes that candidate from the current route, so
other eligible OCG accounts can still be selected.

Removing an OCG-managed CPA runtime is different: it deletes that owned
installation, its local runtime configuration, and the CPA OAuth credentials
under the managed `auth/` directory. It never deletes files belonging to an
externally operated CPA.

## Adding another integration

Static external integrations are one kind of non-core surface that can appear
in **Extensions**. They require product approval, a typed Dashboard V3 adapter,
and a documented local boundary. Dynamic Provider plugins, user scripts,
generic management proxies, and runtime adapter loading are not supported.

---

[User guide index](../USER.md) · [简体中文](external-integrations.zh-CN.md) · [Docs index](../README.md)

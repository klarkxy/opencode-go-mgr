[简体中文](external-integrations.zh-CN.md)

# External Integrations

External integrations are optional, locally supported services that extend OCG
Manager without becoming a Provider, a Plan, or a plugin. The dashboard keeps
its seven core views; supported surfaces appear in the general **Extensions**
group below the Settings divider.

## CPA

CPA (CLI Proxy API) is a local subscription runtime. OCG Manager can manage
its supported Codex, Claude, Antigravity, Kimi, and xAI account flows and route
the resulting subscription pool, but CPA remains the owner of OAuth browser
sessions, tokens, auth files, and internal scheduling. OCG stores only its
local connection configuration, the two CPA access credentials, and a local
model snapshot.

Use one of these local deployments:

- **Desktop or CLI:** run CPA on the same machine and configure a loopback URL
  such as `http://127.0.0.1:8317`.
- **Docker:** enable the optional Compose sibling described in
  [Docker](docker.md). OCG uses the read-only `http://cpa:8317` service URL;
  the dashboard does not accept a LAN, Internet, or cross-node CPA address.

CPA is intentionally not a remote integration. URLs with embedded credentials,
queries, fragments, redirects, or non-loopback hosts are rejected. Do not
reuse an OCG Manager Key as either CPA key.

### Connect and operate

1. Install and start CPA locally, then create its distinct **Management Key**
   and **Inference Key**.
2. Open **Extensions → CPA**, save the local address and both keys,
   then run the connection test. It reports reachability, supported CPA version,
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

## Adding another integration

Static external integrations are one kind of non-core surface that can appear
in **Extensions**. They require product approval, a typed Dashboard V3 adapter,
and a documented local boundary. Dynamic Provider plugins, user scripts,
generic management proxies, and runtime adapter loading are not supported.

---

[User guide index](../USER.md) · [简体中文](external-integrations.zh-CN.md) · [Docs index](../README.md)

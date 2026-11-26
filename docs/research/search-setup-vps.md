# Search-Tool Setup on the VPS (cauce/gateway via Bifrost MCP)

Audited: 2026-11-26. Scope: how web/search MCP access is wired for the coding
harnesses on this machine, and closing gaps for devin, opencode, and agy.

## What "search" actually is here

There is no per-harness search API key sprawl. All harnesses route search/web
tools through a single local MCP gateway:

- **Endpoint**: `http://127.0.0.1:8317/mcp` (Streamable HTTP)
- **Process**: `bifrost-http -port 8317 -host localhost -app-dir ~/.config/bifrost`,
  launched by `~/.own-agents/services/bifrost-inner.sh` under a sops wrapper
  (`sops exec-env ~/.secrets/bifrost.env.sops`).
- **Auth**: HTTP header `x-bf-vk: <BIFROST_VK>`. The secret value is defined in
  `~/.config/environment.d/t3-provider.conf` (systemd user-environment file) and
  is **never** printed or committed.
- **Tools exposed**: `calendar-*` (discover/execute/search-doc across servers
  incl. brave search, firecrawl, tavily), `posthog-exec`, `fetch url`.
- **Config store**: `~/.config/bifrost/config.db` (sqlite; not hand-edited).

Health check: `curl -s http://127.0.0.1:8317/health` → `{"status":"ok"}`.

## Per-harness status

| Harness | Config path | Pattern | Status |
|---|---|---|---|
| codex | `~/.codex/config.toml` | `[mcp_servers.bifrost]` + `env_http_headers` | ✅ pre-existing, verified |
| maki | `~/.config/maki/mcp.toml` | `[mcp.bifrost]` + `headers` w/ `${BIFROST_VK}` | ✅ pre-existing, verified |
| claude | `~/.claude.json` / `~/.claude/settings.json` | `mcpServers` | ⚠️ **not configured** (see Gaps) |
| devin | `~/.config/devin/mcp_config.json` | `mcpServers.bifrost`, literal header | ✅ added this session |
| opencode | `~/.config/opencode/opencode.jsonc` → `/opt/shared-config/opencode/opencode.jsonc` | `mcp.servers.bifrost`, `{env:...}` header | ✅ added + verified |
| agy (Antigravity CLI) | `~/.gemini/config/mcp_config.json` | `mcpServers.bifrost` remote URL | ⚠️ added, unverified (quota) |

## Reference: canonical redacted config snippets

### codex (`~/.codex/config.toml`)

```toml
[mcp_servers.bifrost]
url = "http://127.0.0.1:8317/mcp"
env_http_headers = { "x-bf-vk" = "BIFROST_VK" }
```

### maki (`~/.config/maki/mcp.toml`)

```toml
[mcp.bifrost]
url = "http://127.0.0.1:8317/mcp"
headers = { "x-bf-vk" = "${BIFROST_VK}" }
```

### devin (`~/.config/devin/mcp_config.json`, mode 600) — added

```json
{
  "mcpServers": {
    "bifrost": {
      "url": "http://127.0.0.1:8317/mcp",
      "transport": "http",
      "headers": { "x-bf-vk": "<literal token; devin does not expand env vars>" }
    }
  }
}
```

Created via:

```sh
devin mcp add --transport http --scope user \
  -H "x-bf-vk: $BIFROST_VK" bifrost http://127.0.0.1:8317/mcp
chmod 600 ~/.config/devin/mcp_config.json
```

### opencode (`/opt/shared-config/opencode/opencode.jsonc`) — added

```jsonc
"mcp": {
  "servers": {
    "bifrost": {
      "type": "remote",
      "url": "http://127.0.0.1:8317/mcp",
      "headers": { "x-bf-vk": "{env:BIFROST_VK}" }
    }
  }
}
```

Notes: opencode only interpolates `{env:VAR}` / `{file:path}` (not `${VAR}`) in
remote headers. `~/.config/opencode` is a symlink to `/opt/shared-config/opencode`
(root:shared, 664 file), so a literal token would be world-readable — hence the
env-var form. A literal token was briefly written there during `opencode mcp add`
and was replaced with the placeholder.

### agy (`~/.gemini/config/mcp_config.json`) — added

```json
{
  "mcpServers": {
    "bifrost": {
      "url": "http://127.0.0.1:8317/mcp",
      "headers": { "x-bf-vk": "${BIFROST_VK}" }
    }
  }
}
```

The file existed but was 0 bytes. Per its bundled docs
(`~/.gemini/antigravity-cli/builtin/skills/agy-customizations/docs/mcp_servers.md`),
global config is `~/.gemini/config/mcp_config.json`; remote servers use `url`.
Whether agy expands env-var placeholders in headers is **unconfirmed** — if not,
the header must be a literal token (set file mode 600 first).

## Verification commands

```sh
# Gateway up
curl -s http://127.0.0.1:8317/health

# codex
codex mcp list                       # bifrost  http://127.0.0.1:8317/mcp  enabled

# maki (this session uses it; tools visible)
# calendar-* / posthog-exec / fetch-url tools present

# devin
devin mcp get bifrost                # prints URL + "x-bf-vk: <redacted>"

# opencode (end-to-end, verified working)
opencode run "call the bifrost MCP tool calendar-discover_server_categories_or_actions \
  with user_query 'search', server_names ['google calendar']"

# agy (blocked at audit time)
agy --print "list your available MCP tools"

# Raw endpoint check (200 = good vk, 401 = bad)
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:8317/mcp \
  -H "x-bf-vk: $BIFROST_VK" -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

`BIFROST_VK` is not in the interactive shell env; source it from
`~/.config/environment.d/t3-provider.conf` when needed (do not echo it).

## Remaining gaps

1. **claude**: no bifrost MCP server is configured — `claude mcp list` shows
   only claude.ai built-ins (Docs/Drive/Calendar/Gmail) and `mcpServers` is
   empty in `~/.claude.json`. Left untouched per task scope ("claude presumed
   configured"); fix would be `claude mcp add --transport http -H "x-bf-vk: …"`
   or a project `.mcp.json`.
2. **agy unverified**: Antigravity quota exhausted (429, resets ~115 h) at audit
   time. Re-run the `agy --print` probe after reset; if tools don't appear,
   swap the placeholder for a literal token (chmod 600 first).
3. **Shared config perms**: `/opt/shared-config/opencode/opencode.jsonc` is
   `664 root:shared` (group-writable, world-readable). Safe now that it holds
   only a placeholder, but an admin should tighten to `640`.
4. devin's header necessarily holds the literal token (CLI doesn't interpolate
   env vars); file is mode 600 — acceptable, but revisit if devin adds
   interpolation support.

<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/mcp-proxy/mcp-proxy/refs/heads/main/img/mcp-text-light.svg" alt="mcp-proxy" width="400">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/mcp-proxy/mcp-proxy/refs/heads/main/img/mcp-text-dark.svg" alt="mcp-proxy" width="400">
    <img alt="kagent" src="https://raw.githubusercontent.com/kagent-dev/kagent/main/img/icon-light.svg">
  </picture>
  <div>
     <a href="https://discord.gg/BdJpzaPjHv">
      <img src="https://img.shields.io/discord/1346225185166065826?style=flat&label=Join%20Discord&color=6D28D9" alt="Discord">
    </a>
  </div>
</div>

---


![Build Status](https://github.com/mcp-proxy/mcp-proxy/actions/workflows/pull_request.yml/badge.svg?branch=main)
![Release Status](https://github.com/4t145/rmcp/actions/workflows/release.yml/badge.svg)

**mcp-proxy** is a full-featured enterprise-grade proxy for the MCP protocol.

**Key Features:**

- [x] **Highly performant:** mcp-proxy is written in rust, and is designed from the ground up to handle any scale you can throw at it.
- [x] **Security First:** mcp-proxy includes a robust MCP focused RBAC system.
- [x] **Multi Tenant:** mcp-proxy supports multiple tenants, each with their own set of resources and users.

<br>


# Getting Started 
**Build**

```bash
cargo build
```

**Run**

Local config file
```bash
cargo run -- -f /home/eitanyarmush/src/kagent-dev/mcp-relay/examples/config/static.json
```

Remote config file
```bash
cargo run -- -f https://raw.githubusercontent.com/mcp-proxy/mcp-proxy/main/examples/config/static.json
```

**Test**

```bash
npx @modelcontextprotocol/inspector
```

# mcp-gw

![Build Status](https://github.com/mcp-gw/mcp-gw/actions/workflows/pull_request.yml/badge.svg?branch=main)
![Release Status](https://github.com/4t145/rmcp/actions/workflows/release.yml/badge.svg)

**mcp-gw** is a full-featured enterprise-grade proxy for the MCP protocol.

**Key Features:**

- [x] **Highly performant:** mcp-gw is written in rust, and is designed from the ground up to handle any scale you can throw at it.
- [x] **Security First:** mcp-gw includes a robust MCP focused RBAC system.
- [x] **Multi Tenant:** mcp-gw supports multiple tenants, each with their own set of resources and users.

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
cargo run -- -f https://raw.githubusercontent.com/mcp-gw/mcp-gw/main/examples/config/static.json
```

**Test**

```bash
npx @modelcontextprotocol/inspector
```

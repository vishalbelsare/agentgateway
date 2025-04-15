## Admin API

The admin API is a simple HTTP API that allows you to manage the control plane.

### Running the admin API

```bash
cargo run -- -f examples/admin/config.json
```

## API

First thing we're going to do is create a listener. This will create a basic MCP SSE listener on port 3000.
```bash
curl -X POST -H content-type:application/json http://localhost:19000/listeners -d '{"name": "sse", "sse": {"address": "0.0.0.0", "port": 3000}}'
```

Now that we have a listener, we can create a target. This will create a basic MCP target on port 3000.

```bash
curl -X POST -H content-type:application/json http://localhost:19000/targets/mcp -d '{"name": "everything", "stdio": {"cmd": "npx", "args": ["@modelcontextprotocol/server-everything"]}}'
```

Now we can query the server to get the active config.

```bash
curl http://localhost:19000/listeners
[{"name":"sse","sse":{"address":"0.0.0.0","port":3000}}]
```

We can also get the targets.

```bash
curl http://localhost:19000/targets/mcp
[{"name":"everything","spec":{"Stdio":{"cmd":"npx","args":["@modelcontextprotocol/server-everything"]}}}]
```






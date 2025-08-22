# Schemas
This folder contains JSON schemas for various parts of the project

## Configuration File

|Field|Description|
|-|-|
|`config`||
|`config.enableIpv6`||
|`config.localXdsPath`|Local XDS path. If not specified, the current configuration file will be used.|
|`config.caAddress`||
|`config.xdsAddress`||
|`config.namespace`||
|`config.gateway`||
|`config.trustDomain`||
|`config.serviceAccount`||
|`config.clusterId`||
|`config.network`||
|`config.adminAddr`|Admin UI address in the format "ip:port"|
|`config.statsAddr`|Stats/metrics server address in the format "ip:port"|
|`config.readinessAddr`|Readiness probe server address in the format "ip:port"|
|`config.authToken`||
|`config.connectionTerminationDeadline`||
|`config.connectionMinTerminationDeadline`||
|`config.workerThreads`||
|`config.tracing`||
|`config.tracing.otlpEndpoint`||
|`config.tracing.headers`||
|`config.tracing.otlpProtocol`||
|`config.tracing.fields`||
|`config.tracing.fields.remove`||
|`config.tracing.fields.add`||
|`config.tracing.randomSampling`|Expression to determine the amount of *random sampling*.<br>Random sampling will initiate a new trace span if the incoming request does not have a trace already.<br>This should evaluate to either a float between 0.0-1.0 (0-100%) or true/false.<br>This defaults to 'false'.|
|`config.tracing.clientSampling`|Expression to determine the amount of *client sampling*.<br>Client sampling determines whether to initiate a new trace span if the incoming request does have a trace already.<br>This should evaluate to either a float between 0.0-1.0 (0-100%) or true/false.<br>This defaults to 'true'.|
|`config.logging`||
|`config.logging.filter`||
|`config.logging.fields`||
|`config.logging.fields.remove`||
|`config.logging.fields.add`||
|`config.metrics`||
|`config.metrics.fields`||
|`config.metrics.fields.add`||
|`config.http2`||
|`config.http2.windowSize`||
|`config.http2.connectionWindowSize`||
|`config.http2.frameSize`||
|`config.http2.poolMaxStreamsPerConn`||
|`config.http2.poolUnusedReleaseTimeout`||
|`binds`||
|`binds[].port`||
|`binds[].listeners`||
|`binds[].listeners[].name`||
|`binds[].listeners[].gatewayName`||
|`binds[].listeners[].hostname`|Can be a wildcard|
|`binds[].listeners[].protocol`||
|`binds[].listeners[].tls`||
|`binds[].listeners[].tls.cert`||
|`binds[].listeners[].tls.key`||
|`binds[].listeners[].routes`||
|`binds[].listeners[].routes[].name`||
|`binds[].listeners[].routes[].ruleName`||
|`binds[].listeners[].routes[].hostnames`|Can be a wildcard|
|`binds[].listeners[].routes[].matches`||
|`binds[].listeners[].routes[].matches[].headers`||
|`binds[].listeners[].routes[].matches[].headers[].name`||
|`binds[].listeners[].routes[].matches[].headers[].value`||
|`binds[].listeners[].routes[].matches[].headers[].value.(1)exact`||
|`binds[].listeners[].routes[].matches[].headers[].value.(1)regex`||
|`binds[].listeners[].routes[].matches[].path`||
|`binds[].listeners[].routes[].matches[].path.(1)exact`||
|`binds[].listeners[].routes[].matches[].path.(1)pathPrefix`||
|`binds[].listeners[].routes[].matches[].path.(1)regex`||
|`binds[].listeners[].routes[].matches[].method`||
|`binds[].listeners[].routes[].matches[].query`||
|`binds[].listeners[].routes[].matches[].query[].name`||
|`binds[].listeners[].routes[].matches[].query[].value`||
|`binds[].listeners[].routes[].matches[].query[].value.(1)exact`||
|`binds[].listeners[].routes[].matches[].query[].value.(1)regex`||
|`binds[].listeners[].routes[].policies`||
|`binds[].listeners[].routes[].policies.requestHeaderModifier`|Headers to be modified in the request.|
|`binds[].listeners[].routes[].policies.requestHeaderModifier.add`||
|`binds[].listeners[].routes[].policies.requestHeaderModifier.set`||
|`binds[].listeners[].routes[].policies.requestHeaderModifier.remove`||
|`binds[].listeners[].routes[].policies.responseHeaderModifier`|Headers to be modified in the response.|
|`binds[].listeners[].routes[].policies.responseHeaderModifier.add`||
|`binds[].listeners[].routes[].policies.responseHeaderModifier.set`||
|`binds[].listeners[].routes[].policies.responseHeaderModifier.remove`||
|`binds[].listeners[].routes[].policies.requestRedirect`|Directly respond to the request with a redirect.|
|`binds[].listeners[].routes[].policies.requestRedirect.scheme`||
|`binds[].listeners[].routes[].policies.requestRedirect.authority`||
|`binds[].listeners[].routes[].policies.requestRedirect.authority.(any)(1)full`||
|`binds[].listeners[].routes[].policies.requestRedirect.authority.(any)(1)host`||
|`binds[].listeners[].routes[].policies.requestRedirect.authority.(any)(1)port`||
|`binds[].listeners[].routes[].policies.requestRedirect.path`||
|`binds[].listeners[].routes[].policies.requestRedirect.path.(any)(1)full`||
|`binds[].listeners[].routes[].policies.requestRedirect.path.(any)(1)prefix`||
|`binds[].listeners[].routes[].policies.requestRedirect.status`||
|`binds[].listeners[].routes[].policies.urlRewrite`|Modify the URL path or authority.|
|`binds[].listeners[].routes[].policies.urlRewrite.authority`||
|`binds[].listeners[].routes[].policies.urlRewrite.authority.(any)(1)full`||
|`binds[].listeners[].routes[].policies.urlRewrite.authority.(any)(1)host`||
|`binds[].listeners[].routes[].policies.urlRewrite.authority.(any)(1)port`||
|`binds[].listeners[].routes[].policies.urlRewrite.path`||
|`binds[].listeners[].routes[].policies.urlRewrite.path.(any)(1)full`||
|`binds[].listeners[].routes[].policies.urlRewrite.path.(any)(1)prefix`||
|`binds[].listeners[].routes[].policies.requestMirror`|Mirror incoming requests to another destination.|
|`binds[].listeners[].routes[].policies.requestMirror.backend`||
|`binds[].listeners[].routes[].policies.requestMirror.backend.(1)service`||
|`binds[].listeners[].routes[].policies.requestMirror.backend.(1)service.name`||
|`binds[].listeners[].routes[].policies.requestMirror.backend.(1)service.name.namespace`||
|`binds[].listeners[].routes[].policies.requestMirror.backend.(1)service.name.hostname`||
|`binds[].listeners[].routes[].policies.requestMirror.backend.(1)service.port`||
|`binds[].listeners[].routes[].policies.requestMirror.backend.(1)host`||
|`binds[].listeners[].routes[].policies.requestMirror.percentage`||
|`binds[].listeners[].routes[].policies.directResponse`|Directly respond to the request with a static response.|
|`binds[].listeners[].routes[].policies.directResponse.body`||
|`binds[].listeners[].routes[].policies.directResponse.status`||
|`binds[].listeners[].routes[].policies.cors`|Handle CORS preflight requests and append configured CORS headers to applicable requests.|
|`binds[].listeners[].routes[].policies.cors.allowCredentials`||
|`binds[].listeners[].routes[].policies.cors.allowHeaders`||
|`binds[].listeners[].routes[].policies.cors.allowMethods`||
|`binds[].listeners[].routes[].policies.cors.allowOrigins`||
|`binds[].listeners[].routes[].policies.cors.exposeHeaders`||
|`binds[].listeners[].routes[].policies.cors.maxAge`||
|`binds[].listeners[].routes[].policies.mcpAuthorization`|Authorization policies for MCP access.|
|`binds[].listeners[].routes[].policies.mcpAuthorization.rules`||
|`binds[].listeners[].routes[].policies.authorization`|Authorization policies for HTTP access.|
|`binds[].listeners[].routes[].policies.authorization.rules`||
|`binds[].listeners[].routes[].policies.mcpAuthentication`|Authentication for MCP clients.|
|`binds[].listeners[].routes[].policies.mcpAuthentication.issuer`||
|`binds[].listeners[].routes[].policies.mcpAuthentication.audience`||
|`binds[].listeners[].routes[].policies.mcpAuthentication.jwksUrl`||
|`binds[].listeners[].routes[].policies.mcpAuthentication.provider`||
|`binds[].listeners[].routes[].policies.mcpAuthentication.provider.(any)(1)auth0`||
|`binds[].listeners[].routes[].policies.mcpAuthentication.provider.(any)(1)keycloak`||
|`binds[].listeners[].routes[].policies.mcpAuthentication.resourceMetadata`||
|`binds[].listeners[].routes[].policies.mcpAuthentication.resourceMetadata.resource`||
|`binds[].listeners[].routes[].policies.a2a`|Mark this traffic as A2A to enable A2A processing and telemetry.|
|`binds[].listeners[].routes[].policies.ai`|Mark this as LLM traffic to enable LLM processing.|
|`binds[].listeners[].routes[].policies.ai.promptGuard`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.rejection`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.rejection.body`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.rejection.status`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.action`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.action.(1)reject`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.action.(1)reject.response`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.action.(1)reject.response.body`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.action.(1)reject.response.status`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.rules`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.rules[].(any)builtin`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.rules[].(any)pattern`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.regex.rules[].(any)name`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.webhook`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.webhook.target`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.openaiModeration`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.openaiModeration.model`|Model to use. Defaults to `omni-moderation-latest`|
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.openaiModeration.auth`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.openaiModeration.auth.(1)passthrough`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.openaiModeration.auth.(1)key`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.request.openaiModeration.auth.(1)key.(any)file`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.action`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.action.(1)reject`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.action.(1)reject.response`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.action.(1)reject.response.body`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.action.(1)reject.response.status`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.rules`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.rules[].(any)builtin`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.rules[].(any)pattern`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.regex.rules[].(any)name`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.webhook`||
|`binds[].listeners[].routes[].policies.ai.promptGuard.response.webhook.target`||
|`binds[].listeners[].routes[].policies.ai.defaults`||
|`binds[].listeners[].routes[].policies.ai.overrides`||
|`binds[].listeners[].routes[].policies.ai.prompts`||
|`binds[].listeners[].routes[].policies.ai.prompts.append`||
|`binds[].listeners[].routes[].policies.ai.prompts.append.role`||
|`binds[].listeners[].routes[].policies.ai.prompts.append.content`||
|`binds[].listeners[].routes[].policies.ai.prompts.prepend`||
|`binds[].listeners[].routes[].policies.ai.prompts.prepend.role`||
|`binds[].listeners[].routes[].policies.ai.prompts.prepend.content`||
|`binds[].listeners[].routes[].policies.backendTLS`|Send TLS to the backend.|
|`binds[].listeners[].routes[].policies.backendTLS.cert`||
|`binds[].listeners[].routes[].policies.backendTLS.key`||
|`binds[].listeners[].routes[].policies.backendTLS.root`||
|`binds[].listeners[].routes[].policies.backendTLS.hostname`||
|`binds[].listeners[].routes[].policies.backendTLS.insecure`||
|`binds[].listeners[].routes[].policies.backendTLS.insecureHost`||
|`binds[].listeners[].routes[].policies.backendAuth`|Authenticate to the backend.|
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)passthrough`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)key`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)key.(any)file`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)gcp`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)aws`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)aws.(any)accessKeyId`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)aws.(any)secretAccessKey`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)aws.(any)region`||
|`binds[].listeners[].routes[].policies.backendAuth.(any)(1)aws.(any)sessionToken`||
|`binds[].listeners[].routes[].policies.localRateLimit`|Rate limit incoming requests. State is kept local.|
|`binds[].listeners[].routes[].policies.localRateLimit[].maxTokens`||
|`binds[].listeners[].routes[].policies.localRateLimit[].tokensPerFill`||
|`binds[].listeners[].routes[].policies.localRateLimit[].fillInterval`||
|`binds[].listeners[].routes[].policies.localRateLimit[].type`||
|`binds[].listeners[].routes[].policies.remoteRateLimit`|Rate limit incoming requests. State is managed by a remote server.|
|`binds[].listeners[].routes[].policies.remoteRateLimit.(any)(1)service`||
|`binds[].listeners[].routes[].policies.remoteRateLimit.(any)(1)service.name`||
|`binds[].listeners[].routes[].policies.remoteRateLimit.(any)(1)service.name.namespace`||
|`binds[].listeners[].routes[].policies.remoteRateLimit.(any)(1)service.name.hostname`||
|`binds[].listeners[].routes[].policies.remoteRateLimit.(any)(1)service.port`||
|`binds[].listeners[].routes[].policies.remoteRateLimit.(any)(1)host`||
|`binds[].listeners[].routes[].policies.jwtAuth`|Authenticate incoming JWT requests.|
|`binds[].listeners[].routes[].policies.jwtAuth.mode`||
|`binds[].listeners[].routes[].policies.jwtAuth.issuer`||
|`binds[].listeners[].routes[].policies.jwtAuth.audiences`||
|`binds[].listeners[].routes[].policies.jwtAuth.jwks`||
|`binds[].listeners[].routes[].policies.jwtAuth.jwks.(any)file`||
|`binds[].listeners[].routes[].policies.jwtAuth.jwks.(any)url`||
|`binds[].listeners[].routes[].policies.extAuthz`|Authenticate incoming requests by calling an external authorization server.|
|`binds[].listeners[].routes[].policies.extAuthz.(any)(1)service`||
|`binds[].listeners[].routes[].policies.extAuthz.(any)(1)service.name`||
|`binds[].listeners[].routes[].policies.extAuthz.(any)(1)service.name.namespace`||
|`binds[].listeners[].routes[].policies.extAuthz.(any)(1)service.name.hostname`||
|`binds[].listeners[].routes[].policies.extAuthz.(any)(1)service.port`||
|`binds[].listeners[].routes[].policies.extAuthz.(any)(1)host`||
|`binds[].listeners[].routes[].policies.transformations`|Modify requests and responses|
|`binds[].listeners[].routes[].policies.transformations.request`||
|`binds[].listeners[].routes[].policies.transformations.request.add`||
|`binds[].listeners[].routes[].policies.transformations.request.set`||
|`binds[].listeners[].routes[].policies.transformations.request.remove`||
|`binds[].listeners[].routes[].policies.transformations.request.body`||
|`binds[].listeners[].routes[].policies.transformations.response`||
|`binds[].listeners[].routes[].policies.transformations.response.add`||
|`binds[].listeners[].routes[].policies.transformations.response.set`||
|`binds[].listeners[].routes[].policies.transformations.response.remove`||
|`binds[].listeners[].routes[].policies.transformations.response.body`||
|`binds[].listeners[].routes[].policies.timeout`|Timeout requests that exceed the configured duration.|
|`binds[].listeners[].routes[].policies.timeout.requestTimeout`||
|`binds[].listeners[].routes[].policies.timeout.backendRequestTimeout`||
|`binds[].listeners[].routes[].policies.retry`|Retry matching requests.|
|`binds[].listeners[].routes[].policies.retry.attempts`||
|`binds[].listeners[].routes[].policies.retry.backoff`||
|`binds[].listeners[].routes[].policies.retry.codes`||
|`binds[].listeners[].routes[].backends`||
|`binds[].listeners[].routes[].backends[].(1)service`||
|`binds[].listeners[].routes[].backends[].(1)service.name`||
|`binds[].listeners[].routes[].backends[].(1)service.name.namespace`||
|`binds[].listeners[].routes[].backends[].(1)service.name.hostname`||
|`binds[].listeners[].routes[].backends[].(1)service.port`||
|`binds[].listeners[].routes[].backends[].(1)host`||
|`binds[].listeners[].routes[].backends[].(1)dynamic`||
|`binds[].listeners[].routes[].backends[].(1)mcp`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)sse`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)sse.host`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)sse.port`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)sse.path`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)mcp`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)mcp.host`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)mcp.port`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)mcp.path`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)stdio`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)stdio.cmd`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)stdio.args`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)stdio.env`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)openapi`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)openapi.host`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)openapi.port`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)openapi.path`||
|`binds[].listeners[].routes[].backends[].(1)mcp.targets[].(1)openapi.schema`||
|`binds[].listeners[].routes[].backends[].(1)mcp.statefulMode`||
|`binds[].listeners[].routes[].backends[].(1)ai`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)openAI`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)openAI.model`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)gemini`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)gemini.model`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)vertex`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)vertex.model`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)vertex.region`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)vertex.projectId`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)anthropic`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)anthropic.model`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)bedrock`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)bedrock.model`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)bedrock.region`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)bedrock.guardrailIdentifier`||
|`binds[].listeners[].routes[].backends[].(1)ai.provider.(1)bedrock.guardrailVersion`||
|`binds[].listeners[].routes[].backends[].(1)ai.hostOverride`||
|`binds[].listeners[].routes[].backends[].(1)ai.tokenize`|Whether to tokenize on the request flow. This enables us to do more accurate rate limits,<br>since we know (part of) the cost of the request upfront.<br>This comes with the cost of an expensive operation.|
|`binds[].listeners[].tcpRoutes`||
|`binds[].listeners[].tcpRoutes[].name`||
|`binds[].listeners[].tcpRoutes[].ruleName`||
|`binds[].listeners[].tcpRoutes[].hostnames`|Can be a wildcard|
|`binds[].listeners[].tcpRoutes[].policies`||
|`binds[].listeners[].tcpRoutes[].policies.backendTls`||
|`binds[].listeners[].tcpRoutes[].policies.backendTls.cert`||
|`binds[].listeners[].tcpRoutes[].policies.backendTls.key`||
|`binds[].listeners[].tcpRoutes[].policies.backendTls.root`||
|`binds[].listeners[].tcpRoutes[].policies.backendTls.hostname`||
|`binds[].listeners[].tcpRoutes[].policies.backendTls.insecure`||
|`binds[].listeners[].tcpRoutes[].policies.backendTls.insecureHost`||
|`binds[].listeners[].tcpRoutes[].backends`||
|`binds[].listeners[].tcpRoutes[].backends[].weight`||
|`binds[].listeners[].tcpRoutes[].backends[].backend`||
|`binds[].listeners[].tcpRoutes[].backends[].backend.(1)service`||
|`binds[].listeners[].tcpRoutes[].backends[].backend.(1)service.name`||
|`binds[].listeners[].tcpRoutes[].backends[].backend.(1)service.name.namespace`||
|`binds[].listeners[].tcpRoutes[].backends[].backend.(1)service.name.hostname`||
|`binds[].listeners[].tcpRoutes[].backends[].backend.(1)service.port`||
|`binds[].listeners[].tcpRoutes[].backends[].backend.(1)host`||
|`workloads`||
|`services`||
## CEL context

|Field|Description|
|-|-|
|`request`|`request` contains attributes about the incoming HTTP request|
|`request.method`|The HTTP method of the request.|
|`request.uri`|The URI of the request.|
|`request.path`|The path of the request URI.|
|`request.headers`|The headers of the request.|
|`request.body`|The body of the request. Warning: accessing the body will cause the body to be buffered.|
|`response`|`response` contains attributes about the HTTP response|
|`response.code`|The HTTP status code of the response.|
|`jwt`|`jwt` contains the claims from a verified JWT token. This is only present if the JWT policy is enabled.|
|`llm`|`llm` contains attributes about an LLM request or response. This is only present when using an `ai` backend.|
|`llm.streaming`|Whether the LLM response is streamed.|
|`llm.requestModel`|The model requested for the LLM request. This may differ from the actual model used.|
|`llm.responseModel`|The model that actually served the LLM response.|
|`llm.provider`|The provider of the LLM.|
|`llm.inputTokens`|The number of tokens in the input/prompt.|
|`llm.outputTokens`|The number of tokens in the output/completion.|
|`llm.totalTokens`|The total number of tokens for the request.|
|`llm.prompt`|The prompt sent to the LLM. Warning: accessing this has some performance impacts for large prompts.|
|`llm.prompt[].role`||
|`llm.prompt[].content`||
|`llm.completion`|The completion from the LLM. Warning: accessing this has some performance impacts for large responses.|
|`llm.params`|The parameters for the LLM request.|
|`llm.params.temperature`||
|`llm.params.top_p`||
|`llm.params.frequency_penalty`||
|`llm.params.presence_penalty`||
|`llm.params.seed`||
|`llm.params.max_tokens`||
|`source`|`source` contains attributes about the source of the request.|
|`source.address`|The IP address of the downstream connection.|
|`source.port`|The port of the downstream connection.|
|`source.identity`|The (Istio SPIFFE) identity of the downstream connection, if available.|
|`source.identity.trustDomain`|The trust domain of the identity.|
|`source.identity.namespace`|The namespace of the identity.|
|`source.identity.serviceAccount`|The service account of the identity.|
|`mcp`|`mcp` contains attributes about the MCP request.|
|`mcp.(any)(1)tool`||
|`mcp.(any)(1)tool.target`|The target of the resource|
|`mcp.(any)(1)tool.name`|The name of the resource|
|`mcp.(any)(1)prompt`||
|`mcp.(any)(1)prompt.target`|The target of the resource|
|`mcp.(any)(1)prompt.name`|The name of the resource|
|`mcp.(any)(1)resource`||
|`mcp.(any)(1)resource.target`|The target of the resource|
|`mcp.(any)(1)resource.name`|The name of the resource|

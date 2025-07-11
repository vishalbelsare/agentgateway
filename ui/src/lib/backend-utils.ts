import { Backend } from "./types";

/**
 * Determine the backend type based on the backend configuration
 */
export function getBackendType(backend: Backend): string {
  if (backend.ai) return "ai";
  if (backend.mcp) return "mcp";
  if (backend.service) return "service";
  if (backend.host) return "host";
  if (backend.dynamic) return "dynamic";
  return "unknown";
}

/**
 * Backend type labels for display
 */
export const BACKEND_TYPE_LABELS = {
  ai: "AI Provider",
  mcp: "MCP Target",
  service: "Service",
  host: "Host",
  dynamic: "Dynamic",
} as const;

/**
 * Available AI providers
 */
export const AI_PROVIDERS = [
  { value: "openAI", label: "OpenAI" },
  { value: "anthropic", label: "Anthropic" },
  { value: "gemini", label: "Gemini" },
  { value: "vertex", label: "Vertex AI" },
  { value: "bedrock", label: "AWS Bedrock" },
] as const;

/**
 * Available MCP target types
 */
export const MCP_TARGET_TYPES = [
  { value: "sse", label: "Server-Sent Events" },
  { value: "stdio", label: "Standard I/O" },
  { value: "openapi", label: "OpenAPI" },
] as const;

/**
 * Default ports for different protocols
 */
export const DEFAULT_PORTS = {
  http: 80,
  https: 443,
  tcp: 80,
} as const;

/**
 * Ensure a port is included in an address string
 */
export function ensurePortInAddress(
  address: string,
  defaultPort: number = DEFAULT_PORTS.http
): string {
  if (address.includes(":")) {
    return address;
  }
  return `${address}:${defaultPort}`;
}

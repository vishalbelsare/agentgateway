"use client";

import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Plus,
  Target,
  ChevronDown,
  ChevronRight,
  Trash2,
  Edit,
  Brain,
  Cloud,
  Server,
  Globe,
  Loader2,
} from "lucide-react";
import { Backend, Route, Listener, Bind } from "@/lib/types";
import { updateConfig, fetchConfig } from "@/lib/api";
import { useServer } from "@/lib/server-context";
import { toast } from "sonner";
import {
  getBackendType,
  AI_PROVIDERS,
  MCP_TARGET_TYPES,
  ensurePortInAddress,
} from "@/lib/backend-utils";

interface BackendWithContext {
  backend: Backend;
  route: Route;
  listener: Listener;
  bind: Bind;
  backendIndex: number;
  routeIndex: number;
}

export function BackendConfig() {
  const { refreshListeners } = useServer();
  const [binds, setBinds] = useState<Bind[]>([]);
  const [expandedBinds, setExpandedBinds] = useState<Set<number>>(new Set());
  const [backends, setBackends] = useState<BackendWithContext[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Dialog states
  const [isAddBackendDialogOpen, setIsAddBackendDialogOpen] = useState(false);
  const [selectedBackendType, setSelectedBackendType] = useState<string>("mcp");
  const [editingBackend, setEditingBackend] = useState<BackendWithContext | null>(null);

  // Form states
  const [backendForm, setBackendForm] = useState({
    name: "",
    weight: "1.0",
    // Route selection
    selectedBindPort: "",
    selectedListenerName: "",
    selectedRouteIndex: "",
    // Service backend fields
    serviceNamespace: "",
    serviceHostname: "",
    servicePort: "",
    // Host backend fields
    hostType: "address" as "address" | "hostname",
    hostAddress: "",
    hostHostname: "",
    hostPort: "",
    // MCP backend fields
    mcpTargets: [] as Array<{
      name: string;
      type: "sse" | "mcp" | "stdio" | "openapi";
      // SSE/MCP/OpenAPI fields
      host: string;
      port: string;
      path: string;
      // URL field for easier SSE/MCP/OpenAPI configuration
      fullUrl: string;
      // Stdio fields
      cmd: string;
      args: string;
      env: string;
      // OpenAPI schema placeholder
      schema: boolean;
    }>,
    // AI backend fields
    aiProvider: "openAI" as "openAI" | "gemini" | "vertex" | "anthropic" | "bedrock",
    aiModel: "",
    aiRegion: "",
    aiProjectId: "",
    aiHostOverrideType: "none" as "none" | "address" | "hostname",
    aiHostAddress: "",
    aiHostHostname: "",
    aiHostPort: "",
  });

  // Get available routes for selection
  const getAvailableRoutes = () => {
    const routes: Array<{
      bindPort: number;
      listenerName: string;
      routeIndex: number;
      routeName: string;
      path: string;
    }> = [];

    binds.forEach((bind) => {
      bind.listeners.forEach((listener) => {
        listener.routes?.forEach((route, routeIndex) => {
          const routeName = route.name || `Route ${routeIndex + 1}`;
          const path = route.matches?.[0]?.path
            ? route.matches[0].path.exact || route.matches[0].path.pathPrefix || "/*"
            : "/*";

          routes.push({
            bindPort: bind.port,
            listenerName: listener.name || "unnamed",
            routeIndex,
            routeName,
            path,
          });
        });
      });
    });

    return routes;
  };

  const resetBackendForm = () => {
    setBackendForm({
      name: "",
      weight: "1",
      selectedBindPort: "",
      selectedListenerName: "",
      selectedRouteIndex: "",
      serviceNamespace: "",
      serviceHostname: "",
      servicePort: "",
      hostType: "address",
      hostAddress: "",
      hostHostname: "",
      hostPort: "",
      mcpTargets: [],
      aiProvider: "openAI",
      aiModel: "",
      aiRegion: "",
      aiProjectId: "",
      aiHostOverrideType: "none",
      aiHostAddress: "",
      aiHostHostname: "",
      aiHostPort: "",
    });
    setSelectedBackendType("mcp");
    setEditingBackend(null);
  };

  const populateFormFromBackend = (backendContext: BackendWithContext) => {
    const { backend, bind, listener, routeIndex } = backendContext;
    const backendType = getBackendType(backend);

    setSelectedBackendType(backendType);
    setBackendForm({
      name: getBackendName(backend),
      weight: String(backend.weight || 1),
      selectedBindPort: String(bind.port),
      selectedListenerName: listener.name || "unnamed",
      selectedRouteIndex: String(routeIndex),

      serviceNamespace: backend.service?.name?.namespace || "",
      serviceHostname: backend.service?.name?.hostname || "",
      servicePort: String(backend.service?.port || ""),

      hostType: (() => {
        const hostStr = typeof backend.host === "string" ? backend.host : "";
        return hostStr.includes(":") ? "hostname" : "address";
      })(),
      hostAddress: typeof backend.host === "string" ? backend.host : "",
      hostHostname: (() => {
        const hostStr = typeof backend.host === "string" ? backend.host : "";
        return hostStr.includes(":") ? hostStr.split(":")[0] : "";
      })(),
      hostPort: (() => {
        const hostStr = typeof backend.host === "string" ? backend.host : "";
        return hostStr.includes(":") ? hostStr.split(":")[1] : "";
      })(),

      mcpTargets:
        backend.mcp?.targets?.map((target) => {
          const baseTarget = {
            name: target.name,
            type: "sse" as const,
            host: "",
            port: "",
            path: "",
            fullUrl: "",
            cmd: "",
            args: "",
            env: "",
            schema: true,
          };

          if (target.sse) {
            const fullUrl = `http://${target.sse.host}:${target.sse.port}${target.sse.path}`;
            return {
              ...baseTarget,
              type: "sse" as const,
              host: target.sse.host,
              port: String(target.sse.port),
              path: target.sse.path,
              fullUrl,
            };
          } else if (target.mcp) {
            const fullUrl = `http://${target.mcp.host}:${target.mcp.port}${target.mcp.path}`;
            return {
              ...baseTarget,
              type: "mcp" as const,
              host: target.mcp.host,
              port: String(target.mcp.port),
              path: target.mcp.path,
              fullUrl,
            };
          } else if (target.stdio) {
            return {
              ...baseTarget,
              type: "stdio" as const,
              cmd: target.stdio.cmd,
              args: target.stdio.args?.join(", ") || "",
              env: Object.entries(target.stdio.env || {})
                .map(([k, v]) => `${k}=${v}`)
                .join(", "),
            };
          } else if (target.openapi) {
            const fullUrl = `http://${target.openapi.host}:${target.openapi.port}`;
            return {
              ...baseTarget,
              type: "openapi" as const,
              host: target.openapi.host,
              port: String(target.openapi.port),
              path: "",
              fullUrl,
              schema: target.openapi.schema,
            };
          }
          return baseTarget;
        }) || [],
      // AI backend
      aiProvider: backend.ai?.provider ? (Object.keys(backend.ai.provider)[0] as any) : "openAI",
      aiModel: backend.ai?.provider ? Object.values(backend.ai.provider)[0]?.model || "" : "",
      aiRegion: backend.ai?.provider ? Object.values(backend.ai.provider)[0]?.region || "" : "",
      aiProjectId: backend.ai?.provider
        ? Object.values(backend.ai.provider)[0]?.projectId || ""
        : "",
      aiHostOverrideType: backend.ai?.hostOverride?.Address
        ? "address"
        : backend.ai?.hostOverride?.Hostname
          ? "hostname"
          : "none",
      aiHostAddress: backend.ai?.hostOverride?.Address || "",
      aiHostHostname: backend.ai?.hostOverride?.Hostname?.[0] || "",
      aiHostPort: String(backend.ai?.hostOverride?.Hostname?.[1] || ""),
    });
  };

  const getBackendName = (backend: Backend): string => {
    if (backend.mcp) return backend.mcp.name;
    if (backend.ai) return backend.ai.name;
    if (backend.service) return backend.service.name.hostname;
    if (backend.host) {
      return typeof backend.host === "string" ? backend.host : String(backend.host);
    }
    if (backend.dynamic) return "Dynamic Backend";
    return "Unknown Backend";
  };

  const getBackendIcon = (type: string) => {
    switch (type) {
      case "mcp":
        return <Target className="h-4 w-4" />;
      case "ai":
        return <Brain className="h-4 w-4" />;
      case "service":
        return <Cloud className="h-4 w-4" />;
      case "host":
        return <Server className="h-4 w-4" />;
      case "dynamic":
        return <Globe className="h-4 w-4" />;
      default:
        return <Server className="h-4 w-4" />;
    }
  };

  const getBackendTypeColor = (type: string): string => {
    switch (type) {
      case "mcp":
        return "bg-blue-500 hover:bg-blue-600";
      case "ai":
        return "bg-green-500 hover:bg-green-600";
      case "service":
        return "bg-orange-500 hover:bg-orange-600";
      case "host":
        return "bg-red-500 hover:bg-red-600";
      case "dynamic":
        return "bg-yellow-500 hover:bg-yellow-600";
      default:
        return "bg-gray-500 hover:bg-gray-600";
    }
  };

  const getBackendDetails = (backend: Backend): { primary: string; secondary?: string } => {
    if (backend.mcp) {
      const targetCount = `${backend.mcp.targets.length} target${backend.mcp.targets.length !== 1 ? "s" : ""}`;

      // Show details for first target if available
      if (backend.mcp.targets.length > 0) {
        const firstTarget = backend.mcp.targets[0];
        if (firstTarget.stdio) {
          const cmd = firstTarget.stdio.cmd;
          const args = firstTarget.stdio.args?.join(" ") || "";
          const fullCmd = args ? `${cmd} ${args}` : cmd;
          return {
            primary: targetCount,
            secondary: fullCmd.length > 60 ? `${fullCmd.substring(0, 60)}...` : fullCmd,
          };
        } else if (firstTarget.sse) {
          const url = `${firstTarget.sse.host}:${firstTarget.sse.port}${firstTarget.sse.path}`;
          return {
            primary: targetCount,
            secondary: url.length > 60 ? `${url.substring(0, 60)}...` : url,
          };
        } else if (firstTarget.mcp) {
          const url = `${firstTarget.mcp.host}:${firstTarget.mcp.port}${firstTarget.mcp.path}`;
          return {
            primary: targetCount,
            secondary: url.length > 60 ? `${url.substring(0, 60)}...` : url,
          };
        } else if (firstTarget.openapi) {
          const url = `${firstTarget.openapi.host}:${firstTarget.openapi.port}`;
          return {
            primary: targetCount,
            secondary: url.length > 60 ? `${url.substring(0, 60)}...` : url,
          };
        }
      }

      return { primary: targetCount };
    }

    if (backend.ai) {
      const provider = Object.keys(backend.ai.provider)[0];
      const config = Object.values(backend.ai.provider)[0] as any;
      const model = config?.model;

      return {
        primary: `Provider: ${provider}`,
        secondary: model ? `Model: ${model}` : undefined,
      };
    }

    if (backend.service) {
      return {
        primary: `Service: ${backend.service.name.hostname}`,
        secondary: `Port: ${backend.service.port}`,
      };
    }

    if (backend.host) {
      const hostStr = typeof backend.host === "string" ? backend.host : String(backend.host);
      if (hostStr.includes(":")) {
        const [hostname, port] = hostStr.split(":");
        return {
          primary: `Host: ${hostname}`,
          secondary: `Port: ${port}`,
        };
      }
      return { primary: `Address: ${hostStr}` };
    }

    if (backend.dynamic) {
      return { primary: "Dynamic routing" };
    }

    return { primary: "" };
  };

  const getBackendsByBind = () => {
    const backendsByBind = new Map<number, BackendWithContext[]>();
    backends.forEach((backendContext) => {
      const port = backendContext.bind.port;
      if (!backendsByBind.has(port)) {
        backendsByBind.set(port, []);
      }
      backendsByBind.get(port)!.push(backendContext);
    });
    return backendsByBind;
  };

  const handleEditBackend = (backendContext: BackendWithContext) => {
    setEditingBackend(backendContext);
    populateFormFromBackend(backendContext);
    setIsAddBackendDialogOpen(true);
  };

  const handleDeleteBackend = async (backendContext: BackendWithContext) => {
    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find and remove the backend
      const bind = config.binds.find((b) => b.port === backendContext.bind.port);
      const listener = bind?.listeners.find((l) => l.name === backendContext.listener.name);
      const route = listener?.routes?.[backendContext.routeIndex];

      if (route?.backends) {
        route.backends.splice(backendContext.backendIndex, 1);
      }

      await updateConfig(config);
      await loadBackends();
      await refreshListeners();

      toast.success("Backend deleted successfully");
    } catch (err) {
      console.error("Error deleting backend:", err);
      toast.error("Failed to delete backend");
    } finally {
      setIsSubmitting(false);
    }
  };

  const addMcpTarget = () => {
    setBackendForm((prev) => ({
      ...prev,
      mcpTargets: [
        ...prev.mcpTargets,
        {
          name: "",
          type: "sse",
          host: "",
          port: "",
          path: "",
          fullUrl: "",
          cmd: "",
          args: "",
          env: "",
          schema: true,
        },
      ],
    }));
  };

  const removeMcpTarget = (index: number) => {
    setBackendForm((prev) => ({
      ...prev,
      mcpTargets: prev.mcpTargets.filter((_, i) => i !== index),
    }));
  };

  const updateMcpTarget = (index: number, field: string, value: any) => {
    setBackendForm((prev) => ({
      ...prev,
      mcpTargets: prev.mcpTargets.map((target, i) =>
        i === index ? { ...target, [field]: value } : target
      ),
    }));
  };

  const parseAndUpdateUrl = (index: number, url: string) => {
    try {
      const urlObj = new URL(url);
      const host = urlObj.hostname;
      const port = urlObj.port || (urlObj.protocol === "https:" ? "443" : "80");
      const path = urlObj.pathname + urlObj.search;

      setBackendForm((prev) => ({
        ...prev,
        mcpTargets: prev.mcpTargets.map((target, i) =>
          i === index
            ? {
                ...target,
                fullUrl: url,
                host,
                port,
                path,
              }
            : target
        ),
      }));
    } catch (err) {
      // Invalid URL, just update the fullUrl field
      setBackendForm((prev) => ({
        ...prev,
        mcpTargets: prev.mcpTargets.map((target, i) =>
          i === index ? { ...target, fullUrl: url } : target
        ),
      }));
    }
  };

  const validateCommonFields = (): boolean => {
    if (!backendForm.name.trim()) return false;
    // Only validate route selection when adding (not editing)
    if (!editingBackend && (!backendForm.selectedBindPort || !backendForm.selectedRouteIndex))
      return false;

    // Validate weight is a positive integer
    const weight = parseInt(backendForm.weight);
    if (isNaN(weight) || weight < 0) return false;

    return true;
  };

  const validateServiceBackend = (): boolean => {
    return !!(
      backendForm.serviceNamespace.trim() &&
      backendForm.serviceHostname.trim() &&
      backendForm.servicePort.trim()
    );
  };

  const validateHostBackend = (): boolean => {
    if (backendForm.hostType === "address") {
      return !!backendForm.hostAddress.trim();
    } else {
      return !!(backendForm.hostHostname.trim() && backendForm.hostPort.trim());
    }
  };

  const validateMcpBackend = (): boolean => {
    if (backendForm.mcpTargets.length === 0) return false;
    return backendForm.mcpTargets.every((target) => {
      if (!target.name.trim()) return false;
      if (target.type === "stdio") {
        return !!target.cmd.trim();
      } else {
        // For SSE/MCP/OpenAPI, check if URL is provided and parsed correctly
        return !!(target.fullUrl.trim() && target.host.trim() && target.port.trim());
      }
    });
  };

  const validateAiBackend = (): boolean => {
    if (backendForm.aiProvider === "vertex" && !backendForm.aiProjectId.trim()) return false;
    if (
      backendForm.aiProvider === "bedrock" &&
      (!backendForm.aiModel.trim() || !backendForm.aiRegion.trim())
    )
      return false;
    return true;
  };

  const validateForm = (): boolean => {
    if (!validateCommonFields()) return false;

    switch (selectedBackendType) {
      case "service":
        return validateServiceBackend();
      case "host":
        return validateHostBackend();
      case "mcp":
        return validateMcpBackend();
      case "ai":
        return validateAiBackend();
      case "dynamic":
        return true;
      default:
        return false;
    }
  };

  const addWeightIfNeeded = (backend: any, weight: number): any => {
    if (weight !== 1) backend.weight = weight;
    return backend;
  };

  const createServiceBackend = (weight: number): Backend => {
    return addWeightIfNeeded(
      {
        service: {
          name: {
            namespace: backendForm.serviceNamespace,
            hostname: backendForm.serviceHostname,
          },
          port: parseInt(backendForm.servicePort),
        },
      },
      weight
    );
  };

  const createHostBackend = (weight: number): Backend => {
    return addWeightIfNeeded(
      {
        host:
          backendForm.hostType === "address"
            ? ensurePortInAddress(backendForm.hostAddress)
            : `${backendForm.hostHostname}:${backendForm.hostPort || "80"}`,
      },
      weight
    );
  };

  const createMcpTarget = (target: any) => {
    const baseTarget = {
      name: target.name,
      filters: [], // Target filters if needed
    };

    switch (target.type) {
      case "sse":
        return {
          ...baseTarget,
          sse: {
            host: target.host,
            port: parseInt(target.port),
            path: target.path,
          },
        };
      case "mcp":
        return {
          ...baseTarget,
          mcp: {
            host: target.host,
            port: parseInt(target.port),
            path: target.path,
          },
        };
      case "stdio":
        return {
          ...baseTarget,
          stdio: {
            cmd: target.cmd,
            args: target.args ? target.args.split(",").map((arg: string) => arg.trim()) : [],
            env: target.env
              ? Object.fromEntries(
                  target.env
                    .split(",")
                    .map((pair: string) => {
                      const [key, value] = pair.split("=");
                      return [key?.trim(), value?.trim()];
                    })
                    .filter(([key, value]: [string, string]) => key && value)
                )
              : {},
          },
        };
      case "openapi":
        return {
          ...baseTarget,
          openapi: {
            host: target.host,
            port: parseInt(target.port),
            schema: target.schema,
          },
        };
      default:
        return baseTarget;
    }
  };

  const createMcpBackend = (weight: number): Backend => {
    const targets = backendForm.mcpTargets.map(createMcpTarget);
    return addWeightIfNeeded(
      {
        mcp: {
          name: backendForm.name,
          targets,
        },
      },
      weight
    );
  };

  const createAiProviderConfig = () => {
    const provider: any = {};

    switch (backendForm.aiProvider) {
      case "openAI":
        provider.openAI = backendForm.aiModel ? { model: backendForm.aiModel } : {};
        break;
      case "gemini":
        provider.gemini = backendForm.aiModel ? { model: backendForm.aiModel } : {};
        break;
      case "vertex":
        provider.vertex = {
          projectId: backendForm.aiProjectId,
          ...(backendForm.aiModel && { model: backendForm.aiModel }),
          ...(backendForm.aiRegion && { region: backendForm.aiRegion }),
        };
        break;
      case "anthropic":
        provider.anthropic = backendForm.aiModel ? { model: backendForm.aiModel } : {};
        break;
      case "bedrock":
        provider.bedrock = {
          model: backendForm.aiModel,
          region: backendForm.aiRegion,
        };
        break;
    }

    return provider;
  };

  const createAiBackend = (weight: number): Backend => {
    const aiConfig: any = {
      name: backendForm.name,
      provider: createAiProviderConfig(),
    };

    // Add host override if specified
    if (backendForm.aiHostOverrideType === "address") {
      aiConfig.hostOverride = { Address: ensurePortInAddress(backendForm.aiHostAddress) };
    } else if (backendForm.aiHostOverrideType === "hostname") {
      aiConfig.hostOverride = {
        Hostname: [backendForm.aiHostHostname, parseInt(backendForm.aiHostPort || "80")],
      };
    }

    return addWeightIfNeeded({ ai: aiConfig }, weight);
  };

  const createDynamicBackend = (weight: number): Backend => {
    return addWeightIfNeeded({ dynamic: {} }, weight);
  };

  const createBackendFromForm = (): Backend => {
    const weight = parseInt(backendForm.weight) || 1;

    switch (selectedBackendType) {
      case "service":
        return createServiceBackend(weight);
      case "host":
        return createHostBackend(weight);
      case "mcp":
        return createMcpBackend(weight);
      case "ai":
        return createAiBackend(weight);
      case "dynamic":
        return createDynamicBackend(weight);
      default:
        throw new Error(`Unknown backend type: ${selectedBackendType}`);
    }
  };

  const handleAddBackend = async () => {
    if (!validateForm()) {
      const weight = parseInt(backendForm.weight);
      if (isNaN(weight) || weight < 0) {
        toast.error("Weight must be a positive integer");
      } else {
        toast.error("Please fill in all required fields");
      }
      return;
    }

    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Use editing backend's route info or form values
      const bindPort = editingBackend
        ? editingBackend.bind.port
        : parseInt(backendForm.selectedBindPort);
      const routeIndex = editingBackend
        ? editingBackend.routeIndex
        : parseInt(backendForm.selectedRouteIndex);
      const listenerName = editingBackend
        ? editingBackend.listener.name
        : backendForm.selectedListenerName;

      // Find the target bind and route
      const bindIndex = config.binds.findIndex((b) => b.port === bindPort);
      if (bindIndex === -1) {
        throw new Error("Selected bind not found");
      }

      const listenerIndex = config.binds[bindIndex].listeners.findIndex(
        (l) => (l.name || "unnamed") === listenerName
      );
      if (listenerIndex === -1) {
        throw new Error("Selected listener not found");
      }

      if (!config.binds[bindIndex].listeners[listenerIndex].routes?.[routeIndex]) {
        throw new Error("Selected route not found");
      }

      // Create the new backend
      const newBackend = createBackendFromForm();

      if (editingBackend) {
        // Edit existing backend
        const route = config.binds[bindIndex].listeners[listenerIndex].routes![routeIndex];
        if (route.backends) {
          route.backends[editingBackend.backendIndex] = newBackend;
        }

        toast.success(
          `${selectedBackendType.toUpperCase()} backend "${backendForm.name}" updated successfully`
        );
      } else {
        // Add new backend
        if (!config.binds[bindIndex].listeners[listenerIndex].routes![routeIndex].backends) {
          config.binds[bindIndex].listeners[listenerIndex].routes![routeIndex].backends = [];
        }
        config.binds[bindIndex].listeners[listenerIndex].routes![routeIndex].backends.push(
          newBackend
        );

        toast.success(
          `${selectedBackendType.toUpperCase()} backend "${backendForm.name}" added successfully`
        );
      }

      // Update the configuration
      await updateConfig(config);

      // Refresh the data
      await loadBackends();
      refreshListeners();

      resetBackendForm();
      setIsAddBackendDialogOpen(false);
    } catch (err) {
      console.error(`Error ${editingBackend ? "updating" : "adding"} backend:`, err);
      toast.error(`Failed to ${editingBackend ? "update" : "add"} backend`);
    } finally {
      setIsSubmitting(false);
    }
  };

  const loadBackends = async () => {
    setIsLoading(true);
    try {
      const config = await fetchConfig();
      const binds = config.binds;
      setBinds(binds);
      const backends: BackendWithContext[] = [];

      binds.forEach((bind) => {
        bind.listeners.forEach((listener) => {
          listener.routes?.forEach((route, routeIndex) => {
            route.backends?.forEach((backend, backendIndex) => {
              backends.push({
                backend,
                route,
                listener,
                bind,
                backendIndex,
                routeIndex,
              });
            });
          });
        });
      });
      setBackends(backends);

      // Auto-expand binds with backends
      const bindsWithBackends = new Set<number>();
      backends.forEach(({ bind }) => bindsWithBackends.add(bind.port));
      setExpandedBinds(bindsWithBackends);
    } catch (err) {
      console.error("Error loading backends:", err);
      toast.error("Failed to load backends");
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadBackends();
  }, []);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary"></div>
        <span className="ml-2">Loading backends...</span>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex justify-end">
        <Button onClick={() => setIsAddBackendDialogOpen(true)}>
          <Plus className="mr-2 h-4 w-4" />
          Add Backend
        </Button>
      </div>

      {backends.length === 0 ? (
        <div className="text-center py-12 border rounded-md bg-muted/20">
          <Target className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
          <p className="text-muted-foreground">No backends configured.</p>
          <p className="text-sm text-muted-foreground mt-2">
            Add backends to your routes to get started.
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {Array.from(getBackendsByBind().entries()).map(([port, backendContexts]) => {
            const typeCounts = backendContexts.reduce(
              (acc, bc) => {
                const type = getBackendType(bc.backend);
                acc[type] = (acc[type] || 0) + 1;
                return acc;
              },
              {} as Record<string, number>
            );

            return (
              <Card key={port}>
                <Collapsible
                  open={expandedBinds.has(port)}
                  onOpenChange={() => {
                    setExpandedBinds((prev) => {
                      const newSet = new Set(prev);
                      if (newSet.has(port)) {
                        newSet.delete(port);
                      } else {
                        newSet.add(port);
                      }
                      return newSet;
                    });
                  }}
                >
                  <CollapsibleTrigger asChild>
                    <CardHeader className="hover:bg-muted/50 cursor-pointer">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center space-x-4">
                          {expandedBinds.has(port) ? (
                            <ChevronDown className="h-4 w-4" />
                          ) : (
                            <ChevronRight className="h-4 w-4" />
                          )}
                          <div>
                            <h3 className="text-lg font-semibold">Port {port}</h3>
                            <div className="flex items-center space-x-4 text-sm text-muted-foreground mt-1">
                              {Object.entries(typeCounts).map(([type, count]) => (
                                <div key={type} className="flex items-center space-x-1">
                                  {getBackendIcon(type)}
                                  <span>
                                    {count} {type.toUpperCase()}
                                  </span>
                                </div>
                              ))}
                            </div>
                          </div>
                        </div>
                        <Badge variant="secondary">{backendContexts.length} backends</Badge>
                      </div>
                    </CardHeader>
                  </CollapsibleTrigger>

                  <CollapsibleContent>
                    <CardContent className="pt-0">
                      <Table>
                        <TableHeader>
                          <TableRow>
                            <TableHead>Name</TableHead>
                            <TableHead>Type</TableHead>
                            <TableHead>Listener</TableHead>
                            <TableHead>Route</TableHead>
                            <TableHead>Details</TableHead>
                            <TableHead>Weight</TableHead>
                            <TableHead className="text-right">Actions</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {backendContexts.map((backendContext, index) => {
                            const type = getBackendType(backendContext.backend);
                            return (
                              <TableRow key={index}>
                                <TableCell className="font-medium">
                                  {getBackendName(backendContext.backend)}
                                </TableCell>
                                <TableCell>
                                  <Badge
                                    variant="secondary"
                                    className={`${getBackendTypeColor(type)} text-white`}
                                  >
                                    {getBackendIcon(type)}
                                    <span className="ml-1 capitalize">{type}</span>
                                  </Badge>
                                </TableCell>
                                <TableCell>
                                  <Badge variant="outline">
                                    {backendContext.listener.name || "unnamed"}
                                  </Badge>
                                </TableCell>
                                <TableCell>
                                  <Badge variant="outline">
                                    {backendContext.route.name ||
                                      `Route ${backendContext.routeIndex + 1}`}
                                  </Badge>
                                </TableCell>
                                <TableCell className="text-sm text-muted-foreground">
                                  {(() => {
                                    const details = getBackendDetails(backendContext.backend);
                                    return (
                                      <div className="space-y-1">
                                        <div>{details.primary}</div>
                                        {details.secondary && (
                                          <div className="text-xs text-muted-foreground/80 font-mono">
                                            {details.secondary}
                                          </div>
                                        )}
                                      </div>
                                    );
                                  })()}
                                </TableCell>
                                <TableCell>
                                  <Badge variant="secondary">
                                    {backendContext.backend.weight || 1}
                                  </Badge>
                                </TableCell>
                                <TableCell className="text-right">
                                  <div className="flex justify-end space-x-2">
                                    <Button
                                      variant="ghost"
                                      size="icon"
                                      onClick={() => handleEditBackend(backendContext)}
                                    >
                                      <Edit className="h-4 w-4" />
                                    </Button>
                                    <Button
                                      variant="ghost"
                                      size="icon"
                                      onClick={() => handleDeleteBackend(backendContext)}
                                      className="text-destructive hover:text-destructive"
                                      disabled={isSubmitting}
                                    >
                                      <Trash2 className="h-4 w-4" />
                                    </Button>
                                  </div>
                                </TableCell>
                              </TableRow>
                            );
                          })}
                        </TableBody>
                      </Table>
                    </CardContent>
                  </CollapsibleContent>
                </Collapsible>
              </Card>
            );
          })}
        </div>
      )}

      {/* Add Backend Dialog */}
      <Dialog open={isAddBackendDialogOpen} onOpenChange={setIsAddBackendDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{editingBackend ? "Edit Backend" : "Add Backend"}</DialogTitle>
            <DialogDescription>
              {editingBackend
                ? "Update the backend configuration."
                : "Configure a new backend for your routes."}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {/* Backend Type Selection */}
            <div className="space-y-2">
              <Label>Backend Type *</Label>
              <div className="grid grid-cols-2 gap-2">
                {[
                  { value: "mcp", label: "MCP", icon: Target },
                  { value: "ai", label: "AI", icon: Brain },
                  { value: "service", label: "Service", icon: Cloud },
                  { value: "host", label: "Host", icon: Server },
                  { value: "dynamic", label: "Dynamic", icon: Globe },
                ].map(({ value, label, icon: Icon }) => (
                  <Button
                    key={value}
                    type="button"
                    variant={selectedBackendType === value ? "default" : "outline"}
                    onClick={() => setSelectedBackendType(value)}
                    className="justify-start"
                  >
                    <Icon className="mr-2 h-4 w-4" />
                    {label}
                  </Button>
                ))}
              </div>
            </div>

            {/* Common fields */}
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="backend-name">Name *</Label>
                <Input
                  id="backend-name"
                  value={backendForm.name}
                  onChange={(e) => setBackendForm((prev) => ({ ...prev, name: e.target.value }))}
                  placeholder="Backend name"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="backend-weight">Weight</Label>
                <Input
                  id="backend-weight"
                  type="number"
                  min="0"
                  step="1"
                  value={backendForm.weight}
                  onChange={(e) => setBackendForm((prev) => ({ ...prev, weight: e.target.value }))}
                  placeholder="1"
                />
                <p className="text-xs text-muted-foreground">
                  Weight determines load balancing priority. Higher values get more traffic.
                </p>
              </div>
            </div>

            {/* Route Selection */}
            <div className="space-y-2">
              <Label>Route *</Label>
              {editingBackend ? (
                <div className="p-3 bg-muted rounded-md">
                  <p className="text-sm">
                    Port {editingBackend.bind.port} → {editingBackend.listener.name || "unnamed"} →{" "}
                    {editingBackend.route.name || `Route ${editingBackend.routeIndex + 1}`}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    Route cannot be changed when editing
                  </p>
                </div>
              ) : (
                <Select
                  value={`${backendForm.selectedBindPort}-${backendForm.selectedListenerName}-${backendForm.selectedRouteIndex}`}
                  onValueChange={(value) => {
                    const [bindPort, listenerName, routeIndex] = value.split("-");
                    setBackendForm((prev) => ({
                      ...prev,
                      selectedBindPort: bindPort,
                      selectedListenerName: listenerName,
                      selectedRouteIndex: routeIndex,
                    }));
                  }}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select a route" />
                  </SelectTrigger>
                  <SelectContent>
                    {getAvailableRoutes().length === 0 ? (
                      <div className="py-2 px-3 text-sm text-muted-foreground">
                        No routes available. Create a route first.
                      </div>
                    ) : (
                      getAvailableRoutes().map((route) => (
                        <SelectItem
                          key={`${route.bindPort}-${route.listenerName}-${route.routeIndex}`}
                          value={`${route.bindPort}-${route.listenerName}-${route.routeIndex}`}
                        >
                          Port {route.bindPort} → {route.listenerName} → {route.routeName} (
                          {route.path})
                        </SelectItem>
                      ))
                    )}
                  </SelectContent>
                </Select>
              )}
            </div>

            {/* Service Backend Configuration */}
            {selectedBackendType === "service" && (
              <div className="space-y-4">
                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="service-namespace">Namespace *</Label>
                    <Input
                      id="service-namespace"
                      value={backendForm.serviceNamespace}
                      onChange={(e) =>
                        setBackendForm((prev) => ({ ...prev, serviceNamespace: e.target.value }))
                      }
                      placeholder="default"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="service-hostname">Hostname *</Label>
                    <Input
                      id="service-hostname"
                      value={backendForm.serviceHostname}
                      onChange={(e) =>
                        setBackendForm((prev) => ({ ...prev, serviceHostname: e.target.value }))
                      }
                      placeholder="my-service"
                    />
                  </div>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="service-port">Port *</Label>
                  <Input
                    id="service-port"
                    type="number"
                    min="0"
                    max="65535"
                    value={backendForm.servicePort}
                    onChange={(e) =>
                      setBackendForm((prev) => ({ ...prev, servicePort: e.target.value }))
                    }
                    placeholder="80"
                  />
                </div>
              </div>
            )}

            {/* Host Backend Configuration */}
            {selectedBackendType === "host" && (
              <div className="space-y-4">
                <div className="space-y-2">
                  <Label>Host Type *</Label>
                  <div className="flex space-x-4">
                    <Button
                      type="button"
                      variant={backendForm.hostType === "address" ? "default" : "outline"}
                      onClick={() => setBackendForm((prev) => ({ ...prev, hostType: "address" }))}
                    >
                      Direct Address
                    </Button>
                    <Button
                      type="button"
                      variant={backendForm.hostType === "hostname" ? "default" : "outline"}
                      onClick={() => setBackendForm((prev) => ({ ...prev, hostType: "hostname" }))}
                    >
                      Hostname + Port
                    </Button>
                  </div>
                </div>

                {backendForm.hostType === "address" ? (
                  <div className="space-y-2">
                    <Label htmlFor="host-address">Address *</Label>
                    <Input
                      id="host-address"
                      value={backendForm.hostAddress}
                      onChange={(e) =>
                        setBackendForm((prev) => ({ ...prev, hostAddress: e.target.value }))
                      }
                      placeholder="192.168.1.100:8080"
                    />
                  </div>
                ) : (
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                      <Label htmlFor="host-hostname">Hostname *</Label>
                      <Input
                        id="host-hostname"
                        value={backendForm.hostHostname}
                        onChange={(e) =>
                          setBackendForm((prev) => ({ ...prev, hostHostname: e.target.value }))
                        }
                        placeholder="example.com"
                      />
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="host-port">Port *</Label>
                      <Input
                        id="host-port"
                        type="number"
                        min="0"
                        max="65535"
                        value={backendForm.hostPort}
                        onChange={(e) =>
                          setBackendForm((prev) => ({ ...prev, hostPort: e.target.value }))
                        }
                        placeholder="8080"
                      />
                    </div>
                  </div>
                )}
              </div>
            )}

            {/* MCP Backend Configuration */}
            {selectedBackendType === "mcp" && (
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <Label>MCP Targets</Label>
                  <Button type="button" variant="outline" size="sm" onClick={addMcpTarget}>
                    <Plus className="mr-1 h-3 w-3" />
                    Add Target
                  </Button>
                </div>

                {backendForm.mcpTargets.map((target, index) => (
                  <Card key={index} className="p-4">
                    <div className="space-y-4">
                      <div className="flex items-center justify-between">
                        <h4 className="text-sm font-medium">Target {index + 1}</h4>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => removeMcpTarget(index)}
                          className="text-destructive hover:text-destructive"
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      </div>

                      <div className="grid grid-cols-2 gap-4">
                        <div className="space-y-2">
                          <Label>Target Name *</Label>
                          <Input
                            value={target.name}
                            onChange={(e) => updateMcpTarget(index, "name", e.target.value)}
                            placeholder="my-target"
                          />
                        </div>
                        <div className="space-y-2">
                          <Label>Target Type *</Label>
                          <Select
                            value={target.type}
                            onValueChange={(value) => updateMcpTarget(index, "type", value)}
                          >
                            <SelectTrigger>
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              {MCP_TARGET_TYPES.map(
                                ({ value, label }: { value: string; label: string }) => (
                                  <SelectItem key={value} value={value}>
                                    {label}
                                  </SelectItem>
                                )
                              )}
                            </SelectContent>
                          </Select>
                        </div>
                      </div>

                      {(target.type === "sse" ||
                        target.type === "mcp" ||
                        target.type === "openapi") && (
                        <div className="space-y-4">
                          <div className="space-y-2">
                            <Label>Full URL *</Label>
                            <Input
                              value={target.fullUrl}
                              onChange={(e) => parseAndUpdateUrl(index, e.target.value)}
                              placeholder="http://localhost:3000/api/mcp"
                            />
                            <p className="text-xs text-muted-foreground">
                              Paste the full URL and it will be automatically parsed into host,
                              port, and path
                            </p>
                          </div>

                          {target.host && target.port && (
                            <div className="p-3 bg-muted/30 rounded-md">
                              <p className="text-sm font-medium mb-2">Parsed Components:</p>
                              <div className="space-y-2">
                                <div className="grid grid-cols-2 gap-4 text-sm">
                                  <div>
                                    <span className="text-muted-foreground">Host:</span>
                                    <span className="ml-2 font-mono">{target.host}</span>
                                  </div>
                                  <div>
                                    <span className="text-muted-foreground">Port:</span>
                                    <span className="ml-2 font-mono">{target.port}</span>
                                  </div>
                                </div>
                                <div className="text-sm">
                                  <span className="text-muted-foreground">Path:</span>
                                  <span
                                    className="ml-2 font-mono truncate block max-w-full"
                                    title={target.path || "/"}
                                  >
                                    {target.path || "/"}
                                  </span>
                                </div>
                              </div>
                            </div>
                          )}
                        </div>
                      )}

                      {target.type === "stdio" && (
                        <div className="space-y-4">
                          <div className="space-y-2">
                            <Label>Command *</Label>
                            <Input
                              value={target.cmd}
                              onChange={(e) => updateMcpTarget(index, "cmd", e.target.value)}
                              placeholder="python3 my_mcp_server.py"
                            />
                          </div>
                          <div className="space-y-2">
                            <Label>Arguments (comma-separated)</Label>
                            <Input
                              value={target.args}
                              onChange={(e) => updateMcpTarget(index, "args", e.target.value)}
                              placeholder="--verbose, --config=/path/to/config"
                            />
                          </div>
                          <div className="space-y-2">
                            <Label>Environment Variables (key=value, comma-separated)</Label>
                            <Input
                              value={target.env}
                              onChange={(e) => updateMcpTarget(index, "env", e.target.value)}
                              placeholder="DEBUG=true, API_KEY=secret"
                            />
                          </div>
                        </div>
                      )}
                    </div>
                  </Card>
                ))}

                {backendForm.mcpTargets.length === 0 && (
                  <div className="text-center py-8 border-2 border-dashed border-muted rounded-md">
                    <Target className="mx-auto h-8 w-8 text-muted-foreground mb-2" />
                    <p className="text-sm text-muted-foreground">No targets configured</p>
                    <p className="text-xs text-muted-foreground">
                      Add at least one target to create an MCP backend
                    </p>
                  </div>
                )}
              </div>
            )}

            {/* AI Backend Configuration */}
            {selectedBackendType === "ai" && (
              <div className="space-y-4">
                <div className="space-y-2">
                  <Label>AI Provider *</Label>
                  <div className="grid grid-cols-3 gap-2">
                    {AI_PROVIDERS.map(({ value, label }: { value: string; label: string }) => (
                      <Button
                        key={value}
                        type="button"
                        variant={backendForm.aiProvider === value ? "default" : "outline"}
                        onClick={() =>
                          setBackendForm((prev) => ({ ...prev, aiProvider: value as any }))
                        }
                        className="text-sm"
                      >
                        {label}
                      </Button>
                    ))}
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="ai-model">
                      Model {backendForm.aiProvider === "bedrock" ? "*" : "(optional)"}
                    </Label>
                    <Input
                      id="ai-model"
                      value={backendForm.aiModel}
                      onChange={(e) =>
                        setBackendForm((prev) => ({ ...prev, aiModel: e.target.value }))
                      }
                      placeholder={
                        backendForm.aiProvider === "openAI"
                          ? "gpt-4"
                          : backendForm.aiProvider === "gemini"
                            ? "gemini-pro"
                            : backendForm.aiProvider === "vertex"
                              ? "gemini-pro"
                              : backendForm.aiProvider === "anthropic"
                                ? "claude-3-sonnet"
                                : "anthropic.claude-3-sonnet"
                      }
                    />
                  </div>

                  {(backendForm.aiProvider === "vertex" ||
                    backendForm.aiProvider === "bedrock") && (
                    <div className="space-y-2">
                      <Label htmlFor="ai-region">
                        Region {backendForm.aiProvider === "bedrock" ? "*" : "(optional)"}
                      </Label>
                      <Input
                        id="ai-region"
                        value={backendForm.aiRegion}
                        onChange={(e) =>
                          setBackendForm((prev) => ({ ...prev, aiRegion: e.target.value }))
                        }
                        placeholder={
                          backendForm.aiProvider === "vertex" ? "us-central1" : "us-east-1"
                        }
                      />
                    </div>
                  )}
                </div>

                {backendForm.aiProvider === "vertex" && (
                  <div className="space-y-2">
                    <Label htmlFor="ai-project-id">Project ID *</Label>
                    <Input
                      id="ai-project-id"
                      value={backendForm.aiProjectId}
                      onChange={(e) =>
                        setBackendForm((prev) => ({ ...prev, aiProjectId: e.target.value }))
                      }
                      placeholder="my-gcp-project"
                    />
                  </div>
                )}

                {/* AI Host Override */}
                <div className="space-y-4">
                  <div className="space-y-2">
                    <Label>Host Override (optional)</Label>
                    <div className="flex space-x-4">
                      <Button
                        type="button"
                        variant={backendForm.aiHostOverrideType === "none" ? "default" : "outline"}
                        onClick={() =>
                          setBackendForm((prev) => ({ ...prev, aiHostOverrideType: "none" }))
                        }
                        size="sm"
                      >
                        None
                      </Button>
                      <Button
                        type="button"
                        variant={
                          backendForm.aiHostOverrideType === "address" ? "default" : "outline"
                        }
                        onClick={() =>
                          setBackendForm((prev) => ({ ...prev, aiHostOverrideType: "address" }))
                        }
                        size="sm"
                      >
                        Address
                      </Button>
                      <Button
                        type="button"
                        variant={
                          backendForm.aiHostOverrideType === "hostname" ? "default" : "outline"
                        }
                        onClick={() =>
                          setBackendForm((prev) => ({ ...prev, aiHostOverrideType: "hostname" }))
                        }
                        size="sm"
                      >
                        Hostname
                      </Button>
                    </div>
                  </div>

                  {backendForm.aiHostOverrideType === "address" && (
                    <div className="space-y-2">
                      <Label htmlFor="ai-host-address">Host Address</Label>
                      <Input
                        id="ai-host-address"
                        value={backendForm.aiHostAddress}
                        onChange={(e) =>
                          setBackendForm((prev) => ({ ...prev, aiHostAddress: e.target.value }))
                        }
                        placeholder="api.custom-ai-provider.com:443"
                      />
                    </div>
                  )}

                  {backendForm.aiHostOverrideType === "hostname" && (
                    <div className="grid grid-cols-2 gap-4">
                      <div className="space-y-2">
                        <Label htmlFor="ai-host-hostname">Hostname</Label>
                        <Input
                          id="ai-host-hostname"
                          value={backendForm.aiHostHostname}
                          onChange={(e) =>
                            setBackendForm((prev) => ({ ...prev, aiHostHostname: e.target.value }))
                          }
                          placeholder="api.custom-ai-provider.com"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label htmlFor="ai-host-port">Port</Label>
                        <Input
                          id="ai-host-port"
                          type="number"
                          min="1"
                          max="65535"
                          value={backendForm.aiHostPort}
                          onChange={(e) =>
                            setBackendForm((prev) => ({ ...prev, aiHostPort: e.target.value }))
                          }
                          placeholder="443"
                        />
                      </div>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Dynamic Backend Configuration */}
            {selectedBackendType === "dynamic" && (
              <div className="p-4 bg-muted/50 rounded-lg">
                <div className="flex items-center space-x-2 mb-2">
                  <Globe className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm font-medium">Dynamic Backend</span>
                </div>
                <p className="text-sm text-muted-foreground">
                  Dynamic backends are automatically configured and don&apos;t require additional
                  settings.
                </p>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setIsAddBackendDialogOpen(false);
                resetBackendForm();
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleAddBackend} disabled={!validateForm() || isSubmitting}>
              {isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {editingBackend ? "Update" : "Add"} {selectedBackendType.toUpperCase()} Backend
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

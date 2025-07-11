"use client";

import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Shield,
  Lock,
  Globe,
  ChevronDown,
  ChevronRight,
  Edit,
  Trash2,
  Plus,
  Settings,
  Timer,
  RotateCcw,
  Key,
  Zap,
  Network,
  FileText,
  ArrowRight,
  Loader2,
  Target,
} from "lucide-react";
import { Route as RouteType, TcpRoute, Listener, Bind } from "@/lib/types";
import { fetchBinds, updateConfig, fetchConfig } from "@/lib/api";
import { useServer } from "@/lib/server-context";
import { toast } from "sonner";

interface RouteWithContext {
  route: RouteType | TcpRoute;
  listener: Listener;
  bind: Bind;
  routeIndex: number;
  routeType: "http" | "tcp";
  routeName: string;
  routePath: string;
}

interface PolicyDialogState {
  isOpen: boolean;
  type: PolicyType | null;
  route: RouteWithContext | null;
  data: any;
}

type PolicyType =
  | "jwtAuth"
  | "mcpAuthentication"
  | "mcpAuthorization"
  | "cors"
  | "backendTLS"
  | "backendAuth"
  | "localRateLimit"
  | "remoteRateLimit"
  | "timeout"
  | "retry"
  | "requestHeaderModifier"
  | "responseHeaderModifier"
  | "requestRedirect"
  | "urlRewrite"
  | "directResponse"
  | "extAuthz"
  | "ai";

interface PolicyTypeInfo {
  name: string;
  icon: React.ElementType;
  description: string;
  httpOnly?: boolean;
  tcpOnly?: boolean;
}

const POLICY_TYPES: Record<PolicyType, PolicyTypeInfo> = {
  jwtAuth: {
    name: "JWT Authentication",
    icon: Shield,
    description: "Validate JWT tokens for authentication",
    httpOnly: true,
  },
  mcpAuthentication: {
    name: "MCP Authentication",
    icon: Key,
    description: "Model Context Protocol authentication",
    httpOnly: true,
  },
  mcpAuthorization: {
    name: "MCP Authorization",
    icon: Lock,
    description: "Model Context Protocol authorization rules",
    httpOnly: true,
  },
  cors: {
    name: "CORS",
    icon: Globe,
    description: "Cross-Origin Resource Sharing configuration",
    httpOnly: true,
  },
  backendTLS: {
    name: "Backend TLS",
    icon: Lock,
    description: "TLS configuration for backend connections",
  },
  backendAuth: {
    name: "Backend Auth",
    icon: Key,
    description: "Authentication for backend services",
  },
  localRateLimit: {
    name: "Local Rate Limit",
    icon: Timer,
    description: "Rate limiting at the gateway level",
    httpOnly: true,
  },
  remoteRateLimit: {
    name: "Remote Rate Limit",
    icon: Network,
    description: "Rate limiting using external service",
    httpOnly: true,
  },
  timeout: {
    name: "Timeout",
    icon: Timer,
    description: "Request and backend timeout configuration",
    httpOnly: true,
  },
  retry: {
    name: "Retry",
    icon: RotateCcw,
    description: "Retry configuration for failed requests",
    httpOnly: true,
  },
  requestHeaderModifier: {
    name: "Request Headers",
    icon: Edit,
    description: "Modify request headers",
    httpOnly: true,
  },
  responseHeaderModifier: {
    name: "Response Headers",
    icon: Edit,
    description: "Modify response headers",
    httpOnly: true,
  },
  requestRedirect: {
    name: "Request Redirect",
    icon: ArrowRight,
    description: "Redirect requests to different URLs",
    httpOnly: true,
  },
  urlRewrite: {
    name: "URL Rewrite",
    icon: Edit,
    description: "Rewrite request URLs",
    httpOnly: true,
  },
  directResponse: {
    name: "Direct Response",
    icon: FileText,
    description: "Return direct responses without backend",
    httpOnly: true,
  },
  extAuthz: {
    name: "External Authorization",
    icon: Shield,
    description: "External authorization service integration",
    httpOnly: true,
  },
  ai: {
    name: "AI Policy",
    icon: Zap,
    description: "AI/LLM policy configuration",
    httpOnly: true,
  },
};

export function PolicyConfig() {
  const { refreshListeners } = useServer();
  const [binds, setBinds] = useState<Bind[]>([]);
  const [expandedBinds, setExpandedBinds] = useState<Set<number>>(new Set());
  const [routes, setRoutes] = useState<RouteWithContext[]>([]);
  const [selectedRoute, setSelectedRoute] = useState<RouteWithContext | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const [policyDialog, setPolicyDialog] = useState<PolicyDialogState>({
    isOpen: false,
    type: null,
    route: null,
    data: null,
  });

  const loadRoutes = async () => {
    setIsLoading(true);
    try {
      const fetchedBinds = await fetchBinds();
      setBinds(fetchedBinds);

      // Extract all routes with context
      const allRoutes: RouteWithContext[] = [];

      fetchedBinds.forEach((bind) => {
        bind.listeners.forEach((listener) => {
          // HTTP routes
          listener.routes?.forEach((route, routeIndex) => {
            const routeName = route.name || `Route ${routeIndex + 1}`;
            const routePath = route.matches?.[0]?.path
              ? route.matches[0].path.exact ||
                route.matches[0].path.pathPrefix ||
                route.matches[0].path.regex?.[0] ||
                "/*"
              : "/*";

            allRoutes.push({
              route,
              listener,
              bind,
              routeIndex,
              routeType: "http",
              routeName,
              routePath,
            });
          });

          // TCP routes
          listener.tcpRoutes?.forEach((tcpRoute, routeIndex) => {
            const routeName = tcpRoute.name || `TCP Route ${routeIndex + 1}`;
            const routePath = tcpRoute.hostnames?.join(", ") || "Any Host";

            allRoutes.push({
              route: tcpRoute,
              listener,
              bind,
              routeIndex,
              routeType: "tcp",
              routeName,
              routePath,
            });
          });
        });
      });

      setRoutes(allRoutes);

      // Auto-expand binds with routes
      const bindsWithRoutes = new Set<number>();
      allRoutes.forEach(({ bind }) => bindsWithRoutes.add(bind.port));
      setExpandedBinds(bindsWithRoutes);
    } catch (err) {
      console.error("Error loading routes:", err);
      toast.error("Failed to load routes");
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadRoutes();
  }, []);

  const getAvailablePolicyTypes = (routeType: "http" | "tcp") => {
    return Object.entries(POLICY_TYPES).filter(([_, info]) => {
      if (routeType === "http") return !info.tcpOnly;
      if (routeType === "tcp") return !info.httpOnly;
      return true;
    });
  };

  const hasPolicyType = (routeContext: RouteWithContext, type: PolicyType) => {
    const policies = routeContext.route.policies;
    if (!policies) return false;
    return (policies as any)[type] !== undefined && (policies as any)[type] !== null;
  };

  const handleAddPolicy = async (routeContext: RouteWithContext, type: PolicyType) => {
    setPolicyDialog({
      isOpen: true,
      type,
      route: routeContext,
      data: getDefaultPolicyData(type),
    });
  };

  const handleEditPolicy = async (routeContext: RouteWithContext, type: PolicyType) => {
    const existingData = (routeContext.route.policies as any)?.[type];
    setPolicyDialog({
      isOpen: true,
      type,
      route: routeContext,
      data: existingData || getDefaultPolicyData(type),
    });
  };

  const handleDeletePolicy = async (routeContext: RouteWithContext, type: PolicyType) => {
    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find and update the route
      const bind = config.binds.find((b) => b.port === routeContext.bind.port);
      const listener = bind?.listeners.find((l) => l.name === routeContext.listener.name);

      if (routeContext.routeType === "http") {
        const route = listener?.routes?.[routeContext.routeIndex];
        if (route && route.policies) {
          delete (route.policies as any)[type];
          // Clean up empty policies object
          if (Object.keys(route.policies).length === 0) {
            delete route.policies;
          }
        }
      } else {
        const tcpRoute = listener?.tcpRoutes?.[routeContext.routeIndex];
        if (tcpRoute && tcpRoute.policies) {
          delete (tcpRoute.policies as any)[type];
          // Clean up empty policies object
          if (Object.keys(tcpRoute.policies).length === 0) {
            delete tcpRoute.policies;
          }
        }
      }

      await updateConfig(config);
      await loadRoutes();
      await refreshListeners();

      toast.success(`${POLICY_TYPES[type].name} policy deleted successfully`);
    } catch (err) {
      console.error("Error deleting policy:", err);
      toast.error("Failed to delete policy");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleSavePolicy = async () => {
    if (!policyDialog.route || !policyDialog.type) return;

    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find and update the route
      const bind = config.binds.find((b) => b.port === policyDialog.route!.bind.port);
      const listener = bind?.listeners.find((l) => l.name === policyDialog.route!.listener.name);

      if (policyDialog.route.routeType === "http") {
        const route = listener?.routes?.[policyDialog.route.routeIndex];
        if (route) {
          if (!route.policies) route.policies = {};
          (route.policies as any)[policyDialog.type] = policyDialog.data;
        }
      } else {
        const tcpRoute = listener?.tcpRoutes?.[policyDialog.route.routeIndex];
        if (tcpRoute) {
          if (!tcpRoute.policies) tcpRoute.policies = {};
          (tcpRoute.policies as any)[policyDialog.type] = policyDialog.data;
        }
      }

      await updateConfig(config);
      await loadRoutes();
      await refreshListeners();

      setPolicyDialog({
        isOpen: false,
        type: null,
        route: null,
        data: null,
      });

      toast.success(`${POLICY_TYPES[policyDialog.type].name} policy saved successfully`);
    } catch (err) {
      console.error("Error saving policy:", err);
      toast.error("Failed to save policy");
    } finally {
      setIsSubmitting(false);
    }
  };

  const getDefaultPolicyData = (type: PolicyType) => {
    switch (type) {
      case "jwtAuth":
        return {
          issuer: "",
          audiences: [],
          jwks: "",
        };
      case "mcpAuthentication":
        return {
          issuer: "",
          scopes: [],
          audience: "",
          provider: null,
        };
      case "mcpAuthorization":
        return {
          rules: [],
        };
      case "cors":
        return {
          allowCredentials: false,
          allowHeaders: [],
          allowMethods: [],
          allowOrigins: [],
          exposeHeaders: [],
          maxAge: null,
        };
      case "backendTLS":
        return {
          cert: null,
          key: null,
          root: null,
          insecure: false,
          insecureHost: false,
        };
      case "backendAuth":
        return {
          passthrough: {},
        };
      case "localRateLimit":
        return [];
      case "remoteRateLimit":
        return {
          target: "",
          descriptors: {},
        };
      case "timeout":
        return {
          requestTimeout: null,
          backendRequestTimeout: null,
        };
      case "retry":
        return {
          attempts: 1,
          backoff: null,
          codes: [],
        };
      case "requestHeaderModifier":
      case "responseHeaderModifier":
        return {
          add: [],
          set: [],
          remove: [],
        };
      case "requestRedirect":
        return {
          scheme: null,
          authority: null,
          path: null,
          status: null,
        };
      case "urlRewrite":
        return {
          authority: null,
          path: null,
        };
      case "directResponse":
        return {
          body: "",
          status: "200",
        };
      case "extAuthz":
        return {
          target: "",
          context: {},
        };
      case "ai":
        return {
          provider: null,
          hostOverride: null,
          promptGuard: null,
        };
      default:
        return {};
    }
  };

  const getRoutesByBind = () => {
    const routesByBind = new Map<number, RouteWithContext[]>();
    routes.forEach((routeContext) => {
      const port = routeContext.bind.port;
      if (!routesByBind.has(port)) {
        routesByBind.set(port, []);
      }
      routesByBind.get(port)!.push(routeContext);
    });
    return routesByBind;
  };

  const getPolicyCount = (routeContext: RouteWithContext) => {
    const policies = routeContext.route.policies;
    if (!policies) return 0;
    return Object.keys(policies).length;
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8">
        <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary"></div>
        <span className="ml-2">Loading routes...</span>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {routes.length === 0 ? (
        <div className="text-center py-12 border rounded-md bg-muted/20">
          <Settings className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
          <p className="text-muted-foreground">No routes found.</p>
          <p className="text-sm text-muted-foreground mt-2">
            Create routes first to configure policies.
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {Array.from(getRoutesByBind().entries()).map(([port, routeContexts]) => (
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
                          <p className="text-sm text-muted-foreground">
                            {routeContexts.length} route{routeContexts.length !== 1 ? "s" : ""}{" "}
                            available
                          </p>
                        </div>
                      </div>
                      <Badge variant="secondary">{routeContexts.length} routes</Badge>
                    </div>
                  </CardHeader>
                </CollapsibleTrigger>

                <CollapsibleContent>
                  <CardContent className="pt-0">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead>Route</TableHead>
                          <TableHead>Type</TableHead>
                          <TableHead>Listener</TableHead>
                          <TableHead>Path/Hostnames</TableHead>
                          <TableHead>Policies</TableHead>
                          <TableHead className="text-right">Actions</TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {routeContexts.map((routeContext, index) => (
                          <TableRow
                            key={index}
                            className={selectedRoute === routeContext ? "bg-muted/50" : ""}
                          >
                            <TableCell className="font-medium">{routeContext.routeName}</TableCell>
                            <TableCell>
                              <Badge
                                variant={
                                  routeContext.routeType === "http" ? "default" : "secondary"
                                }
                              >
                                {routeContext.routeType.toUpperCase()}
                              </Badge>
                            </TableCell>
                            <TableCell>
                              <Badge variant="outline">
                                {routeContext.listener.name || "Unnamed"}
                              </Badge>
                            </TableCell>
                            <TableCell className="font-mono text-sm">
                              {routeContext.routePath}
                            </TableCell>
                            <TableCell>
                              <Badge
                                variant={getPolicyCount(routeContext) > 0 ? "default" : "outline"}
                              >
                                {getPolicyCount(routeContext)} policies
                              </Badge>
                            </TableCell>
                            <TableCell className="text-right">
                              <Button
                                variant={selectedRoute === routeContext ? "default" : "outline"}
                                size="sm"
                                onClick={() => setSelectedRoute(routeContext)}
                              >
                                <Target className="h-4 w-4 mr-2" />
                                {selectedRoute === routeContext ? "Selected" : "Select"}
                              </Button>
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </CardContent>
                </CollapsibleContent>
              </Collapsible>
            </Card>
          ))}
        </div>
      )}

      {/* Policy Configuration Section */}
      {selectedRoute && (
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-lg font-semibold">Configure Policies</h3>
                <p className="text-sm text-muted-foreground">
                  {selectedRoute.routeName} ({selectedRoute.routeType.toUpperCase()}) -{" "}
                  {selectedRoute.routePath}
                </p>
              </div>
              <div className="flex items-center space-x-2">
                <Badge variant="outline">Port {selectedRoute.bind.port}</Badge>
                <Badge variant="outline">{selectedRoute.listener.name || "Unnamed"}</Badge>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {getAvailablePolicyTypes(selectedRoute.routeType).map(([type, info]) => {
                const hasPolicy = hasPolicyType(selectedRoute, type as PolicyType);
                const IconComponent = info.icon;

                return (
                  <div
                    key={type}
                    className="border rounded-lg p-4 hover:bg-muted/50 transition-colors"
                  >
                    <div className="flex items-center justify-between mb-2">
                      <div className="flex items-center space-x-2">
                        <IconComponent className="h-5 w-5 text-muted-foreground" />
                        <span className="font-medium">{info.name}</span>
                      </div>
                      <Badge variant={hasPolicy ? "default" : "outline"} className="text-xs">
                        {hasPolicy ? "Active" : "Inactive"}
                      </Badge>
                    </div>
                    <p className="text-sm text-muted-foreground mb-3">{info.description}</p>
                    <div className="flex items-center space-x-2">
                      <Button
                        variant={hasPolicy ? "secondary" : "default"}
                        size="sm"
                        onClick={() => {
                          if (hasPolicy) {
                            handleEditPolicy(selectedRoute, type as PolicyType);
                          } else {
                            handleAddPolicy(selectedRoute, type as PolicyType);
                          }
                        }}
                        className="flex-1"
                      >
                        {hasPolicy ? (
                          <>
                            <Edit className="h-3 w-3 mr-2" />
                            Edit
                          </>
                        ) : (
                          <>
                            <Plus className="h-3 w-3 mr-2" />
                            Add
                          </>
                        )}
                      </Button>
                      {hasPolicy && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleDeletePolicy(selectedRoute, type as PolicyType)}
                          className="text-destructive hover:text-destructive"
                          disabled={isSubmitting}
                        >
                          <Trash2 className="h-3 w-3" />
                        </Button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Policy Edit Dialog */}
      <Dialog
        open={policyDialog.isOpen}
        onOpenChange={(open: boolean) => {
          if (!open) {
            setPolicyDialog({
              isOpen: false,
              type: null,
              route: null,
              data: null,
            });
          }
        }}
      >
        <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>
              {policyDialog.type && POLICY_TYPES[policyDialog.type]
                ? `Configure ${POLICY_TYPES[policyDialog.type].name}`
                : "Configure Policy"}
            </DialogTitle>
            <DialogDescription>
              {policyDialog.route && (
                <div className="mt-2">
                  <Badge variant="outline" className="mr-2">
                    {policyDialog.route.routeName}
                  </Badge>
                  <Badge variant="outline" className="mr-2">
                    {policyDialog.route.routeType.toUpperCase()}
                  </Badge>
                  <span className="text-sm text-muted-foreground">
                    {policyDialog.route.routePath}
                  </span>
                </div>
              )}
              {policyDialog.type && POLICY_TYPES[policyDialog.type]
                ? POLICY_TYPES[policyDialog.type].description
                : "Configure policy settings"}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {policyDialog.type &&
              renderPolicyForm(policyDialog.type, policyDialog.data || {}, (data) => {
                setPolicyDialog((prev) => ({ ...prev, data }));
              })}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() =>
                setPolicyDialog({
                  isOpen: false,
                  type: null,
                  route: null,
                  data: null,
                })
              }
            >
              Cancel
            </Button>
            <Button onClick={handleSavePolicy} disabled={isSubmitting}>
              {isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Save Policy
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

// Helper function to handle comma-separated array input
const handleArrayInput = (value: string): string[] => {
  return value
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
};

// Helper function to format array as comma-separated string
const formatArrayForInput = (arr: any): string => {
  return Array.isArray(arr) ? arr.join(", ") : arr || "";
};

// Helper function to add default port if not present
const ensurePort = (value: string, defaultPort: string = "80"): string => {
  return value && !value.includes(":") ? `${value}:${defaultPort}` : value;
};

function renderJwtAuthForm(data: any, onChange: (data: any) => void) {
  return (
    <div className="space-y-6">
      <div className="space-y-3">
        <Label htmlFor="issuer">Issuer *</Label>
        <Input
          id="issuer"
          value={data.issuer || ""}
          onChange={(e) => onChange({ ...data, issuer: e.target.value })}
          placeholder="https://example.auth0.com/"
        />
      </div>
      <div className="space-y-3">
        <Label htmlFor="audiences">Audiences (comma-separated) *</Label>
        <Input
          id="audiences"
          value={formatArrayForInput(data.audiences)}
          onChange={(e) =>
            onChange({
              ...data,
              audiences: handleArrayInput(e.target.value),
            })
          }
          placeholder="audience1, audience2"
        />
      </div>
      <div className="space-y-3">
        <Label htmlFor="jwks">JWKS URL or File Path *</Label>
        <Input
          id="jwks"
          value={data.jwks || ""}
          onChange={(e) => onChange({ ...data, jwks: e.target.value })}
          placeholder="https://example.auth0.com/.well-known/jwks.json"
        />
      </div>
    </div>
  );
}

function renderPolicyForm(type: PolicyType, data: any, onChange: (data: any) => void) {
  switch (type) {
    case "jwtAuth":
      return renderJwtAuthForm(data, onChange);

      // Helper component for array input fields with live/blur handling
      const ArrayInput = ({
        id,
        label,
        value,
        onChange,
        placeholder,
      }: {
        id: string;
        label: string;
        value: any;
        onChange: (array: string[]) => void;
        placeholder?: string;
      }) => (
        <div className="space-y-3">
          <Label htmlFor={id}>{label}</Label>
          <Input
            id={id}
            value={formatArrayForInput(value)}
            onChange={(e) => onChange(e.target.value as any)} // Allow string during typing
            onBlur={(e) => onChange(handleArrayInput(e.target.value))}
            placeholder={placeholder}
          />
        </div>
      );

      function renderCorsForm(data: any, onChange: (data: any) => void) {
        return (
          <div className="space-y-6">
            <div className="flex items-center space-x-2">
              <Checkbox
                id="allowCredentials"
                checked={data.allowCredentials || false}
                onCheckedChange={(checked: boolean) =>
                  onChange({ ...data, allowCredentials: checked })
                }
              />
              <Label htmlFor="allowCredentials">Allow Credentials</Label>
            </div>

            <ArrayInput
              id="allowOrigins"
              label="Allow Origins (comma-separated)"
              value={data.allowOrigins}
              onChange={(value) => onChange({ ...data, allowOrigins: value })}
              placeholder="https://example.com, https://app.example.com"
            />

            <ArrayInput
              id="allowMethods"
              label="Allow Methods (comma-separated)"
              value={data.allowMethods}
              onChange={(value) => onChange({ ...data, allowMethods: value })}
              placeholder="GET, POST, PUT, DELETE"
            />

            <ArrayInput
              id="allowHeaders"
              label="Allow Headers (comma-separated)"
              value={data.allowHeaders}
              onChange={(value) => onChange({ ...data, allowHeaders: value })}
              placeholder="Content-Type, Authorization"
            />

            <ArrayInput
              id="exposeHeaders"
              label="Expose Headers (comma-separated)"
              value={data.exposeHeaders}
              onChange={(value) => onChange({ ...data, exposeHeaders: value })}
              placeholder="X-Custom-Header"
            />

            <div className="space-y-3">
              <Label htmlFor="maxAge">Max Age (seconds)</Label>
              <Input
                id="maxAge"
                type="number"
                value={data.maxAge || ""}
                onChange={(e) =>
                  onChange({ ...data, maxAge: e.target.value ? parseInt(e.target.value) : null })
                }
                placeholder="3600"
              />
            </div>
          </div>
        );
      }

    case "cors":
      return renderCorsForm(data, onChange);

      function renderTimeoutForm(data: any, onChange: (data: any) => void) {
        return (
          <div className="space-y-6">
            <div className="space-y-3">
              <Label htmlFor="requestTimeout">
                Request Timeout (e.g., &quot;30s&quot;, &quot;1m&quot;)
              </Label>
              <Input
                id="requestTimeout"
                value={data.requestTimeout || ""}
                onChange={(e) => onChange({ ...data, requestTimeout: e.target.value })}
                placeholder="30s"
              />
            </div>
            <div className="space-y-3">
              <Label htmlFor="backendRequestTimeout">
                Backend Request Timeout (e.g., &quot;30s&quot;, &quot;1m&quot;)
              </Label>
              <Input
                id="backendRequestTimeout"
                value={data.backendRequestTimeout || ""}
                onChange={(e) => onChange({ ...data, backendRequestTimeout: e.target.value })}
                placeholder="15s"
              />
            </div>
          </div>
        );
      }

      function renderRetryForm(data: any, onChange: (data: any) => void) {
        const handleCodesArrayInput = (value: string): number[] => {
          return value
            .split(",")
            .map((s) => parseInt(s.trim()))
            .filter((n) => !isNaN(n));
        };

        return (
          <div className="space-y-6">
            <div className="space-y-3">
              <Label htmlFor="attempts">Max Attempts</Label>
              <Input
                id="attempts"
                type="number"
                min="1"
                max="255"
                value={data.attempts || 1}
                onChange={(e) => onChange({ ...data, attempts: parseInt(e.target.value) || 1 })}
              />
            </div>
            <div className="space-y-3">
              <Label htmlFor="backoff">
                Backoff Duration (e.g., &quot;100ms&quot;, &quot;1s&quot;)
              </Label>
              <Input
                id="backoff"
                value={data.backoff || ""}
                onChange={(e) => onChange({ ...data, backoff: e.target.value })}
                placeholder="100ms"
              />
            </div>
            <div className="space-y-3">
              <Label htmlFor="codes">Retry on HTTP Status Codes (comma-separated)</Label>
              <Input
                id="codes"
                value={formatArrayForInput(data.codes)}
                onChange={(e) => onChange({ ...data, codes: e.target.value })}
                onBlur={(e) => onChange({ ...data, codes: handleCodesArrayInput(e.target.value) })}
                placeholder="500, 502, 503, 504"
              />
            </div>
          </div>
        );
      }

    case "timeout":
      return renderTimeoutForm(data, onChange);

    case "retry":
      return renderRetryForm(data, onChange);

      function renderDirectResponseForm(data: any, onChange: (data: any) => void) {
        return (
          <div className="space-y-6">
            <div className="space-y-3">
              <Label htmlFor="status">HTTP Status Code</Label>
              <Input
                id="status"
                type="number"
                min="100"
                max="599"
                value={data.status || "200"}
                onChange={(e) => onChange({ ...data, status: e.target.value || "200" })}
              />
            </div>
            <div className="space-y-3">
              <Label htmlFor="body">Response Body</Label>
              <Textarea
                id="body"
                value={data.body || ""}
                onChange={(e) => onChange({ ...data, body: e.target.value })}
                placeholder="Response content"
                rows={4}
              />
            </div>
          </div>
        );
      }

    case "directResponse":
      return renderDirectResponseForm(data, onChange);

    case "backendTLS":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label htmlFor="cert">Certificate Path</Label>
            <Input
              id="cert"
              value={data.cert || ""}
              onChange={(e) => onChange({ ...data, cert: e.target.value })}
              placeholder="/path/to/cert.pem"
            />
          </div>
          <div className="space-y-3">
            <Label htmlFor="key">Private Key Path</Label>
            <Input
              id="key"
              value={data.key || ""}
              onChange={(e) => onChange({ ...data, key: e.target.value })}
              placeholder="/path/to/key.pem"
            />
          </div>
          <div className="space-y-3">
            <Label htmlFor="root">CA Certificate Path</Label>
            <Input
              id="root"
              value={data.root || ""}
              onChange={(e) => onChange({ ...data, root: e.target.value })}
              placeholder="/path/to/ca.pem"
            />
          </div>
          <div className="flex items-center space-x-2">
            <Checkbox
              id="insecure"
              checked={data.insecure || false}
              onCheckedChange={(checked: boolean) => onChange({ ...data, insecure: checked })}
            />
            <Label htmlFor="insecure">Skip Certificate Verification</Label>
          </div>
          <div className="flex items-center space-x-2">
            <Checkbox
              id="insecureHost"
              checked={data.insecureHost || false}
              onCheckedChange={(checked: boolean) => onChange({ ...data, insecureHost: checked })}
            />
            <Label htmlFor="insecureHost">Skip Hostname Verification</Label>
          </div>
        </div>
      );

    case "localRateLimit":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label>Rate Limit Rules</Label>
            <div className="space-y-4">
              {Array.isArray(data) && data.length > 0 ? (
                data.map((rule: any, index: number) => (
                  <div key={index} className="border rounded-lg p-4 space-y-4">
                    <div className="flex items-center justify-between">
                      <span className="font-medium">Rule {index + 1}</span>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => {
                          const newRules = [...data];
                          newRules.splice(index, 1);
                          onChange(newRules);
                        }}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                    <div className="grid grid-cols-2 gap-4">
                      <div className="space-y-2">
                        <Label htmlFor={`maxTokens-${index}`}>Max Tokens</Label>
                        <Input
                          id={`maxTokens-${index}`}
                          type="number"
                          min="1"
                          value={rule.maxTokens || ""}
                          onChange={(e) => {
                            const newRules = [...data];
                            newRules[index] = { ...rule, maxTokens: parseInt(e.target.value) || 0 };
                            onChange(newRules);
                          }}
                          placeholder="100"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label htmlFor={`tokensPerFill-${index}`}>Tokens per Fill</Label>
                        <Input
                          id={`tokensPerFill-${index}`}
                          type="number"
                          min="1"
                          value={rule.tokensPerFill || ""}
                          onChange={(e) => {
                            const newRules = [...data];
                            newRules[index] = {
                              ...rule,
                              tokensPerFill: parseInt(e.target.value) || 0,
                            };
                            onChange(newRules);
                          }}
                          placeholder="10"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label htmlFor={`fillInterval-${index}`}>Fill Interval</Label>
                        <Input
                          id={`fillInterval-${index}`}
                          value={rule.fillInterval || ""}
                          onChange={(e) => {
                            const newRules = [...data];
                            newRules[index] = { ...rule, fillInterval: e.target.value };
                            onChange(newRules);
                          }}
                          placeholder="1s"
                        />
                      </div>
                      <div className="space-y-2">
                        <Label htmlFor={`type-${index}`}>Type</Label>
                        <Select
                          value={rule.type || "requests"}
                          onValueChange={(value) => {
                            const newRules = [...data];
                            newRules[index] = { ...rule, type: value };
                            onChange(newRules);
                          }}
                        >
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="requests">Requests</SelectItem>
                            <SelectItem value="tokens">Tokens</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>
                    </div>
                  </div>
                ))
              ) : (
                <div className="text-center py-4 text-muted-foreground">
                  No rate limit rules configured.
                </div>
              )}
            </div>
            <Button
              variant="outline"
              onClick={() => {
                const newRule = {
                  maxTokens: 100,
                  tokensPerFill: 10,
                  fillInterval: "1s",
                  type: "requests",
                };
                onChange([...(Array.isArray(data) ? data : []), newRule]);
              }}
            >
              <Plus className="h-4 w-4 mr-2" />
              Add Rate Limit Rule
            </Button>
          </div>
        </div>
      );

      // Helper component for target input with port defaulting
      const TargetInput = ({
        id,
        label,
        value,
        onChange,
        placeholder,
        required = false,
      }: {
        id: string;
        label: string;
        value: string;
        onChange: (value: string) => void;
        placeholder?: string;
        required?: boolean;
      }) => (
        <div className="space-y-3">
          <Label htmlFor={id}>
            {label} {required && "*"}
          </Label>
          <Input
            id={id}
            value={value || ""}
            onChange={(e) => onChange(e.target.value)}
            onBlur={(e) => {
              const val = e.target.value.trim();
              if (val && !val.includes(":")) {
                onChange(ensurePort(val));
              }
            }}
            placeholder={placeholder}
          />
        </div>
      );

      function renderRemoteRateLimitForm(data: any, onChange: (data: any) => void) {
        const handleDescriptorChange = (key: string, field: string, value: any) => {
          const newDescriptors = { ...data.descriptors };
          if (field === "key") {
            const oldValue = newDescriptors[key];
            delete newDescriptors[key];
            newDescriptors[value] = oldValue;
          } else if (field === "type") {
            if (value === "static") {
              newDescriptors[key] = { static: "" };
            } else if (value === "requestHeader") {
              newDescriptors[key] = "";
            }
          } else if (field === "value") {
            const descriptor = newDescriptors[key];
            const isStatic =
              typeof descriptor === "object" && descriptor !== null && "static" in descriptor;
            if (isStatic) {
              newDescriptors[key] = { static: value };
            } else {
              newDescriptors[key] = value;
            }
          }
          onChange({ ...data, descriptors: newDescriptors });
        };

        const addDescriptor = () => {
          const newDescriptors = { ...data.descriptors };
          const newKey = `descriptor${Object.keys(newDescriptors).length + 1}`;
          newDescriptors[newKey] = "";
          onChange({ ...data, descriptors: newDescriptors });
        };

        const removeDescriptor = (key: string) => {
          const newDescriptors = { ...data.descriptors };
          delete newDescriptors[key];
          onChange({ ...data, descriptors: newDescriptors });
        };

        return (
          <div className="space-y-6">
            <TargetInput
              id="target"
              label="Target (host:port)"
              value={data.target}
              onChange={(value) => onChange({ ...data, target: value })}
              placeholder="ratelimit-service.example.com:8080"
              required
            />

            <div className="space-y-3">
              <Label>Rate Limit Descriptors</Label>
              <p className="text-sm text-muted-foreground">
                Configure descriptors that identify rate limit keys. Each descriptor can use a
                request header or a static value.
              </p>
              {Object.entries(data.descriptors || {}).map(([key, descriptor], index) => {
                const isStatic =
                  typeof descriptor === "object" && descriptor !== null && "static" in descriptor;
                const isRequestHeader = typeof descriptor === "string";

                return (
                  <div key={index} className="border rounded-lg p-4 space-y-3">
                    <div className="flex items-center justify-between">
                      <Label htmlFor={`desc-key-${index}`}>Descriptor {index + 1}</Label>
                      <Button variant="ghost" size="sm" onClick={() => removeDescriptor(key)}>
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>

                    <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                      <div className="space-y-2">
                        <Label htmlFor={`desc-key-${index}`}>Key</Label>
                        <Input
                          id={`desc-key-${index}`}
                          value={key}
                          onChange={(e) => handleDescriptorChange(key, "key", e.target.value)}
                          placeholder="user_type"
                        />
                      </div>

                      <div className="space-y-2">
                        <Label htmlFor={`desc-type-${index}`}>Type</Label>
                        <Select
                          value={isStatic ? "static" : isRequestHeader ? "requestHeader" : ""}
                          onValueChange={(value) => handleDescriptorChange(key, "type", value)}
                        >
                          <SelectTrigger>
                            <SelectValue placeholder="Select type" />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="requestHeader">Request Header</SelectItem>
                            <SelectItem value="static">Static Value</SelectItem>
                          </SelectContent>
                        </Select>
                      </div>

                      <div className="space-y-2">
                        <Label htmlFor={`desc-value-${index}`}>
                          {isStatic ? "Static Value" : "Header Name"}
                        </Label>
                        <Input
                          id={`desc-value-${index}`}
                          value={
                            isStatic
                              ? (descriptor as any).static || ""
                              : isRequestHeader
                                ? (descriptor as string)
                                : ""
                          }
                          onChange={(e) => handleDescriptorChange(key, "value", e.target.value)}
                          placeholder={isStatic ? "premium" : "x-user-type"}
                        />
                      </div>
                    </div>
                  </div>
                );
              })}

              <Button variant="outline" size="sm" onClick={addDescriptor}>
                <Plus className="h-4 w-4 mr-2" />
                Add Descriptor
              </Button>
            </div>
          </div>
        );
      }

    case "remoteRateLimit":
      return renderRemoteRateLimitForm(data, onChange);

    case "mcpAuthentication":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label htmlFor="issuer">Issuer *</Label>
            <Input
              id="issuer"
              value={data.issuer || ""}
              onChange={(e) => onChange({ ...data, issuer: e.target.value })}
              placeholder="https://example.auth0.com/"
            />
          </div>
          <div className="space-y-3">
            <Label htmlFor="scopes">Scopes (comma-separated) *</Label>
            <Input
              id="scopes"
              value={Array.isArray(data.scopes) ? data.scopes.join(", ") : ""}
              onChange={(e) =>
                onChange({
                  ...data,
                  scopes: e.target.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter((s) => s.length > 0),
                })
              }
              placeholder="read:tools, write:tools"
            />
          </div>
          <div className="space-y-3">
            <Label htmlFor="audience">Audience *</Label>
            <Input
              id="audience"
              value={data.audience || ""}
              onChange={(e) => onChange({ ...data, audience: e.target.value })}
              placeholder="mcp-api"
            />
          </div>
          <div className="space-y-3">
            <Label htmlFor="provider">Provider</Label>
            <Select
              value={data.provider ? (data.provider.auth0 ? "auth0" : "keycloak") : ""}
              onValueChange={(value) => {
                const provider =
                  value === "auth0"
                    ? { auth0: {} }
                    : value === "keycloak"
                      ? { keycloak: {} }
                      : null;
                onChange({ ...data, provider });
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select a provider" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auth0">Auth0</SelectItem>
                <SelectItem value="keycloak">Keycloak</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      );

    case "mcpAuthorization":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label>Cedar Authorization Rules</Label>
            <p className="text-sm text-muted-foreground">
              Define Cedar policy rules for MCP authorization. Each rule should be a valid Cedar
              policy.
            </p>
            {(data.rules || []).map((rule: string, index: number) => (
              <div key={index} className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label htmlFor={`rule-${index}`}>Rule {index + 1}</Label>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      const newRules = [...(data.rules || [])];
                      newRules.splice(index, 1);
                      onChange({ ...data, rules: newRules });
                    }}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
                <Textarea
                  id={`rule-${index}`}
                  value={rule || ""}
                  onChange={(e) => {
                    const newRules = [...(data.rules || [])];
                    newRules[index] = e.target.value;
                    onChange({ ...data, rules: newRules });
                  }}
                  placeholder={`permit (
  principal in User::"*",
  action == Action::"call_tool",
  resource in Tool::"*"
);`}
                  rows={4}
                  className="font-mono"
                />
              </div>
            ))}
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                const newRules = [...(data.rules || []), ""];
                onChange({ ...data, rules: newRules });
              }}
            >
              <Plus className="h-4 w-4 mr-2" />
              Add Rule
            </Button>
          </div>
        </div>
      );

      // Helper component for header pair management
      const HeaderPairList = ({
        title,
        headers,
        onChange,
        buttonText,
        namePlaceholder = "Header name",
        valuePlaceholder = "Header value",
      }: {
        title: string;
        headers: [string, string][];
        onChange: (headers: [string, string][]) => void;
        buttonText: string;
        namePlaceholder?: string;
        valuePlaceholder?: string;
      }) => (
        <div className="space-y-3">
          <Label>{title}</Label>
          <div className="space-y-2">
            {headers.map((header, index) => (
              <div key={index} className="flex items-center space-x-2">
                <Input
                  value={header[0] || ""}
                  onChange={(e) => {
                    const newHeaders = [...headers];
                    newHeaders[index] = [e.target.value, header[1] || ""];
                    onChange(newHeaders);
                  }}
                  placeholder={namePlaceholder}
                  className="flex-1"
                />
                <Input
                  value={header[1] || ""}
                  onChange={(e) => {
                    const newHeaders = [...headers];
                    newHeaders[index] = [header[0] || "", e.target.value];
                    onChange(newHeaders);
                  }}
                  placeholder={valuePlaceholder}
                  className="flex-1"
                />
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    const newHeaders = [...headers];
                    newHeaders.splice(index, 1);
                    onChange(newHeaders);
                  }}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
            <Button variant="outline" size="sm" onClick={() => onChange([...headers, ["", ""]])}>
              <Plus className="h-4 w-4 mr-2" />
              {buttonText}
            </Button>
          </div>
        </div>
      );

      // Helper component for simple string list management
      const StringList = ({
        title,
        items,
        onChange,
        buttonText,
        placeholder = "Enter value",
      }: {
        title: string;
        items: string[];
        onChange: (items: string[]) => void;
        buttonText: string;
        placeholder?: string;
      }) => (
        <div className="space-y-3">
          <Label>{title}</Label>
          <div className="space-y-2">
            {items.map((item, index) => (
              <div key={index} className="flex items-center space-x-2">
                <Input
                  value={item || ""}
                  onChange={(e) => {
                    const newItems = [...items];
                    newItems[index] = e.target.value;
                    onChange(newItems);
                  }}
                  placeholder={placeholder}
                  className="flex-1"
                />
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    const newItems = [...items];
                    newItems.splice(index, 1);
                    onChange(newItems);
                  }}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
            <Button variant="outline" size="sm" onClick={() => onChange([...items, ""])}>
              <Plus className="h-4 w-4 mr-2" />
              {buttonText}
            </Button>
          </div>
        </div>
      );

      function renderHeaderModifierForm(data: any, onChange: (data: any) => void) {
        return (
          <div className="space-y-6">
            <HeaderPairList
              title="Add Headers"
              headers={data.add || []}
              onChange={(add) => onChange({ ...data, add })}
              buttonText="Add Header"
            />

            <HeaderPairList
              title="Set Headers"
              headers={data.set || []}
              onChange={(set) => onChange({ ...data, set })}
              buttonText="Set Header"
            />

            <StringList
              title="Remove Headers"
              items={data.remove || []}
              onChange={(remove) => onChange({ ...data, remove })}
              buttonText="Remove Header"
              placeholder="Header name to remove"
            />
          </div>
        );
      }

    case "requestHeaderModifier":
    case "responseHeaderModifier":
      return renderHeaderModifierForm(data, onChange);

    case "requestRedirect":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label htmlFor="scheme">Scheme</Label>
            <Select
              value={data.scheme || ""}
              onValueChange={(value) => onChange({ ...data, scheme: value || null })}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select scheme" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="http">HTTP</SelectItem>
                <SelectItem value="https">HTTPS</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-3">
            <Label>Authority Redirect</Label>
            <RadioGroup
              value={
                data.authority?.full !== undefined
                  ? "full"
                  : data.authority?.host !== undefined
                    ? "host"
                    : data.authority?.port !== undefined
                      ? "port"
                      : "none"
              }
              onValueChange={(value) => {
                switch (value) {
                  case "full":
                    onChange({ ...data, authority: { full: "" } });
                    break;
                  case "host":
                    onChange({ ...data, authority: { host: "" } });
                    break;
                  case "port":
                    onChange({ ...data, authority: { port: 80 } });
                    break;
                  case "none":
                  default:
                    onChange({ ...data, authority: null });
                    break;
                }
              }}
            >
              <div className="space-y-3">
                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="none" id="authorityNone" />
                  <Label htmlFor="authorityNone">No authority redirect</Label>
                </div>

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="full" id="authorityFull" />
                  <Label htmlFor="authorityFull">Full authority (host:port)</Label>
                </div>
                {data.authority?.full !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.authority.full || ""}
                      onChange={(e) => onChange({ ...data, authority: { full: e.target.value } })}
                      placeholder="example.com:8080"
                    />
                  </div>
                )}

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="host" id="authorityHost" />
                  <Label htmlFor="authorityHost">Host only</Label>
                </div>
                {data.authority?.host !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.authority.host || ""}
                      onChange={(e) => onChange({ ...data, authority: { host: e.target.value } })}
                      placeholder="example.com"
                    />
                  </div>
                )}

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="port" id="authorityPort" />
                  <Label htmlFor="authorityPort">Port only</Label>
                </div>
                {data.authority?.port !== undefined && (
                  <div className="ml-6">
                    <Input
                      type="number"
                      min="1"
                      max="65535"
                      value={data.authority.port || ""}
                      onChange={(e) =>
                        onChange({ ...data, authority: { port: parseInt(e.target.value) || 80 } })
                      }
                      placeholder="8080"
                    />
                  </div>
                )}
              </div>
            </RadioGroup>
          </div>

          <div className="space-y-3">
            <Label>Path Redirect</Label>
            <RadioGroup
              value={
                data.path?.full !== undefined
                  ? "full"
                  : data.path?.prefix !== undefined
                    ? "prefix"
                    : "none"
              }
              onValueChange={(value) => {
                switch (value) {
                  case "full":
                    onChange({ ...data, path: { full: "" } });
                    break;
                  case "prefix":
                    onChange({ ...data, path: { prefix: "" } });
                    break;
                  case "none":
                  default:
                    onChange({ ...data, path: null });
                    break;
                }
              }}
            >
              <div className="space-y-3">
                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="none" id="pathNone" />
                  <Label htmlFor="pathNone">No path redirect</Label>
                </div>

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="full" id="pathFull" />
                  <Label htmlFor="pathFull">Full path replacement</Label>
                </div>
                {data.path?.full !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.path.full || ""}
                      onChange={(e) => onChange({ ...data, path: { full: e.target.value } })}
                      placeholder="/new/path"
                    />
                  </div>
                )}

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="prefix" id="pathPrefix" />
                  <Label htmlFor="pathPrefix">Prefix replacement</Label>
                </div>
                {data.path?.prefix !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.path.prefix || ""}
                      onChange={(e) => onChange({ ...data, path: { prefix: e.target.value } })}
                      placeholder="/api"
                    />
                  </div>
                )}
              </div>
            </RadioGroup>
          </div>

          <div className="space-y-3">
            <Label htmlFor="status">HTTP Status Code</Label>
            <Input
              id="status"
              type="number"
              min="300"
              max="399"
              value={data.status || ""}
              onChange={(e) => onChange({ ...data, status: parseInt(e.target.value) || null })}
              placeholder="302"
            />
          </div>
        </div>
      );

    case "urlRewrite":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label>Authority Rewrite</Label>
            <RadioGroup
              value={
                data.authority?.full !== undefined
                  ? "full"
                  : data.authority?.host !== undefined
                    ? "host"
                    : data.authority?.port !== undefined
                      ? "port"
                      : "none"
              }
              onValueChange={(value) => {
                switch (value) {
                  case "full":
                    onChange({ ...data, authority: { full: "" } });
                    break;
                  case "host":
                    onChange({ ...data, authority: { host: "" } });
                    break;
                  case "port":
                    onChange({ ...data, authority: { port: 80 } });
                    break;
                  case "none":
                  default:
                    onChange({ ...data, authority: null });
                    break;
                }
              }}
            >
              <div className="space-y-3">
                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="none" id="rewriteAuthorityNone" />
                  <Label htmlFor="rewriteAuthorityNone">No authority rewrite</Label>
                </div>

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="full" id="rewriteAuthorityFull" />
                  <Label htmlFor="rewriteAuthorityFull">Full authority (host:port)</Label>
                </div>
                {data.authority?.full !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.authority.full || ""}
                      onChange={(e) => onChange({ ...data, authority: { full: e.target.value } })}
                      placeholder="example.com:8080"
                    />
                  </div>
                )}

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="host" id="rewriteAuthorityHost" />
                  <Label htmlFor="rewriteAuthorityHost">Host only</Label>
                </div>
                {data.authority?.host !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.authority.host || ""}
                      onChange={(e) => onChange({ ...data, authority: { host: e.target.value } })}
                      placeholder="example.com"
                    />
                  </div>
                )}

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="port" id="rewriteAuthorityPort" />
                  <Label htmlFor="rewriteAuthorityPort">Port only</Label>
                </div>
                {data.authority?.port !== undefined && (
                  <div className="ml-6">
                    <Input
                      type="number"
                      min="1"
                      max="65535"
                      value={data.authority.port || ""}
                      onChange={(e) =>
                        onChange({ ...data, authority: { port: parseInt(e.target.value) || 80 } })
                      }
                      placeholder="8080"
                    />
                  </div>
                )}
              </div>
            </RadioGroup>
          </div>

          <div className="space-y-3">
            <Label>Path Rewrite</Label>
            <RadioGroup
              value={
                data.path?.full !== undefined
                  ? "full"
                  : data.path?.prefix !== undefined
                    ? "prefix"
                    : "none"
              }
              onValueChange={(value) => {
                switch (value) {
                  case "full":
                    onChange({ ...data, path: { full: "" } });
                    break;
                  case "prefix":
                    onChange({ ...data, path: { prefix: "" } });
                    break;
                  case "none":
                  default:
                    onChange({ ...data, path: null });
                    break;
                }
              }}
            >
              <div className="space-y-3">
                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="none" id="rewritePathNone" />
                  <Label htmlFor="rewritePathNone">No path rewrite</Label>
                </div>

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="full" id="rewritePathFull" />
                  <Label htmlFor="rewritePathFull">Full path replacement</Label>
                </div>
                {data.path?.full !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.path.full || ""}
                      onChange={(e) => onChange({ ...data, path: { full: e.target.value } })}
                      placeholder="/new/path"
                    />
                  </div>
                )}

                <div className="flex items-center space-x-2">
                  <RadioGroupItem value="prefix" id="rewritePathPrefix" />
                  <Label htmlFor="rewritePathPrefix">Prefix replacement</Label>
                </div>
                {data.path?.prefix !== undefined && (
                  <div className="ml-6">
                    <Input
                      value={data.path.prefix || ""}
                      onChange={(e) => onChange({ ...data, path: { prefix: e.target.value } })}
                      placeholder="/api"
                    />
                  </div>
                )}
              </div>
            </RadioGroup>
          </div>
        </div>
      );

    case "backendAuth":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label>Authentication Type</Label>
            <Select
              value={
                data.passthrough
                  ? "passthrough"
                  : data.key
                    ? "key"
                    : data.gcp
                      ? "gcp"
                      : data.aws
                        ? "aws"
                        : ""
              }
              onValueChange={(value) => {
                switch (value) {
                  case "passthrough":
                    onChange({ passthrough: {} });
                    break;
                  case "key":
                    onChange({ key: "" });
                    break;
                  case "gcp":
                    onChange({ gcp: {} });
                    break;
                  case "aws":
                    onChange({ aws: {} });
                    break;
                  default:
                    onChange({});
                }
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select authentication type" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="passthrough">Passthrough</SelectItem>
                <SelectItem value="key">API Key</SelectItem>
                <SelectItem value="gcp">Google Cloud Platform</SelectItem>
                <SelectItem value="aws">Amazon Web Services</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {data.key !== undefined && (
            <div className="space-y-3">
              <Label>API Key</Label>
              <div className="space-y-2">
                <div className="flex items-center space-x-2">
                  <Checkbox
                    id="keyFile"
                    checked={typeof data.key === "object" && data.key?.file !== undefined}
                    onCheckedChange={(checked: boolean) => {
                      if (checked) {
                        onChange({ ...data, key: { file: "" } });
                      } else {
                        onChange({ ...data, key: "" });
                      }
                    }}
                  />
                  <Label htmlFor="keyFile">Load from file</Label>
                </div>
                {typeof data.key === "object" && data.key?.file !== undefined ? (
                  <Input
                    value={data.key.file || ""}
                    onChange={(e) => onChange({ ...data, key: { file: e.target.value } })}
                    placeholder="/path/to/api-key.txt"
                  />
                ) : (
                  <Input
                    value={typeof data.key === "string" ? data.key : ""}
                    onChange={(e) => onChange({ ...data, key: e.target.value })}
                    placeholder="your-api-key"
                    type="password"
                  />
                )}
              </div>
            </div>
          )}
        </div>
      );

      // Helper component for key-value pair management
      const KeyValueManager = ({
        title,
        description,
        data,
        onChange,
        keyPlaceholder = "Key",
        valuePlaceholder = "Value",
        addButtonText = "Add Item",
      }: {
        title: string;
        description?: string;
        data: Record<string, any>;
        onChange: (data: Record<string, any>) => void;
        keyPlaceholder?: string;
        valuePlaceholder?: string;
        addButtonText?: string;
      }) => {
        const addItem = () => {
          const newData = { ...data };
          newData[`key${Object.keys(newData).length + 1}`] = "";
          onChange(newData);
        };

        const removeItem = (key: string) => {
          const newData = { ...data };
          delete newData[key];
          onChange(newData);
        };

        const updateKey = (oldKey: string, newKey: string) => {
          const newData = { ...data };
          const value = newData[oldKey];
          delete newData[oldKey];
          newData[newKey] = value;
          onChange(newData);
        };

        const updateValue = (key: string, value: string) => {
          const newData = { ...data };
          newData[key] = value;
          onChange(newData);
        };

        return (
          <div className="space-y-3">
            <Label>{title}</Label>
            {description && <p className="text-sm text-muted-foreground">{description}</p>}
            {Object.entries(data || {}).map(([key, value], index) => (
              <div key={index} className="flex space-x-2">
                <Input
                  placeholder={keyPlaceholder}
                  value={key}
                  onChange={(e) => updateKey(key, e.target.value)}
                />
                <Input
                  placeholder={valuePlaceholder}
                  value={value as string}
                  onChange={(e) => updateValue(key, e.target.value)}
                />
                <Button variant="ghost" size="sm" onClick={() => removeItem(key)}>
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
            <Button variant="outline" size="sm" onClick={addItem}>
              <Plus className="h-4 w-4 mr-2" />
              {addButtonText}
            </Button>
          </div>
        );
      };

      function renderExtAuthzForm(data: any, onChange: (data: any) => void) {
        return (
          <div className="space-y-6">
            <TargetInput
              id="target"
              label="Target (host:port)"
              value={data.target}
              onChange={(value) => onChange({ ...data, target: value })}
              placeholder="auth-service.example.com:8080"
              required
            />

            <KeyValueManager
              title="Context Extensions"
              description="Additional context key-value pairs sent to the authorization service"
              data={data.context}
              onChange={(context) => onChange({ ...data, context })}
              addButtonText="Add Context"
            />
          </div>
        );
      }

    case "extAuthz":
      return renderExtAuthzForm(data, onChange);

    case "ai":
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label>AI Provider *</Label>
            <Select
              value={
                data.provider?.openAI
                  ? "openai"
                  : data.provider?.gemini
                    ? "gemini"
                    : data.provider?.vertex
                      ? "vertex"
                      : data.provider?.anthropic
                        ? "anthropic"
                        : data.provider?.bedrock
                          ? "bedrock"
                          : ""
              }
              onValueChange={(value) => {
                let provider = null;
                switch (value) {
                  case "openai":
                    provider = { openAI: { model: null } };
                    break;
                  case "gemini":
                    provider = { gemini: { model: null } };
                    break;
                  case "vertex":
                    provider = { vertex: { projectId: "", model: null, region: null } };
                    break;
                  case "anthropic":
                    provider = { anthropic: { model: null } };
                    break;
                  case "bedrock":
                    provider = { bedrock: { model: "", region: "" } };
                    break;
                }
                onChange({ ...data, provider });
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select AI provider" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="openai">OpenAI</SelectItem>
                <SelectItem value="gemini">Google Gemini</SelectItem>
                <SelectItem value="vertex">Google Vertex AI</SelectItem>
                <SelectItem value="anthropic">Anthropic</SelectItem>
                <SelectItem value="bedrock">AWS Bedrock</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Provider-specific configuration */}
          {data.provider?.openAI && (
            <div className="space-y-3">
              <Label htmlFor="openai-model">OpenAI Model</Label>
              <Input
                id="openai-model"
                value={data.provider.openAI.model || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      openAI: {
                        model: e.target.value || null,
                      },
                    },
                  })
                }
                placeholder="gpt-4o, gpt-3.5-turbo, etc. (optional)"
              />
            </div>
          )}

          {data.provider?.gemini && (
            <div className="space-y-3">
              <Label htmlFor="gemini-model">Gemini Model</Label>
              <Input
                id="gemini-model"
                value={data.provider.gemini.model || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      gemini: {
                        model: e.target.value || null,
                      },
                    },
                  })
                }
                placeholder="gemini-pro, gemini-1.5-flash, etc. (optional)"
              />
            </div>
          )}

          {data.provider?.vertex && (
            <div className="space-y-3">
              <Label htmlFor="vertex-project-id">Project ID *</Label>
              <Input
                id="vertex-project-id"
                value={data.provider.vertex.projectId || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      vertex: {
                        ...data.provider.vertex,
                        projectId: e.target.value,
                      },
                    },
                  })
                }
                placeholder="your-gcp-project-id"
              />
              <Label htmlFor="vertex-model">Model</Label>
              <Input
                id="vertex-model"
                value={data.provider.vertex.model || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      vertex: {
                        ...data.provider.vertex,
                        model: e.target.value || null,
                      },
                    },
                  })
                }
                placeholder="gemini-pro, claude-3-opus, etc. (optional)"
              />
              <Label htmlFor="vertex-region">Region</Label>
              <Input
                id="vertex-region"
                value={data.provider.vertex.region || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      vertex: {
                        ...data.provider.vertex,
                        region: e.target.value || null,
                      },
                    },
                  })
                }
                placeholder="us-central1, europe-west1, etc. (optional)"
              />
            </div>
          )}

          {data.provider?.anthropic && (
            <div className="space-y-3">
              <Label htmlFor="anthropic-model">Anthropic Model</Label>
              <Input
                id="anthropic-model"
                value={data.provider.anthropic.model || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      anthropic: {
                        model: e.target.value || null,
                      },
                    },
                  })
                }
                placeholder="claude-3-opus, claude-3-sonnet, etc. (optional)"
              />
            </div>
          )}

          {data.provider?.bedrock && (
            <div className="space-y-3">
              <Label htmlFor="bedrock-model">Model *</Label>
              <Input
                id="bedrock-model"
                value={data.provider.bedrock.model || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      bedrock: {
                        ...data.provider.bedrock,
                        model: e.target.value,
                      },
                    },
                  })
                }
                placeholder="anthropic.claude-3-opus-20240229-v1:0"
              />
              <Label htmlFor="bedrock-region">Region *</Label>
              <Input
                id="bedrock-region"
                value={data.provider.bedrock.region || ""}
                onChange={(e) =>
                  onChange({
                    ...data,
                    provider: {
                      bedrock: {
                        ...data.provider.bedrock,
                        region: e.target.value,
                      },
                    },
                  })
                }
                placeholder="us-east-1, us-west-2, etc."
              />
            </div>
          )}

          {/* Host Override */}
          <div className="space-y-3">
            <Label>Host Override (Optional)</Label>
            <p className="text-sm text-muted-foreground">
              Override the default host for the AI provider
            </p>
            <Select
              value={
                data.hostOverride?.Address
                  ? "address"
                  : data.hostOverride?.Hostname
                    ? "hostname"
                    : "none"
              }
              onValueChange={(value) => {
                let hostOverride = null;
                if (value === "address") {
                  hostOverride = { Address: "" };
                } else if (value === "hostname") {
                  hostOverride = { Hostname: ["", 443] };
                }
                onChange({ ...data, hostOverride });
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder="No host override" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">No host override</SelectItem>
                <SelectItem value="address">IP Address</SelectItem>
                <SelectItem value="hostname">Hostname</SelectItem>
              </SelectContent>
            </Select>

            {data.hostOverride?.Address !== undefined && (
              <div className="ml-6">
                <Label htmlFor="host-address">IP Address</Label>
                <Input
                  id="host-address"
                  value={data.hostOverride.Address || ""}
                  onChange={(e) =>
                    onChange({
                      ...data,
                      hostOverride: { Address: e.target.value },
                    })
                  }
                  onBlur={(e) => {
                    const value = e.target.value.trim();
                    if (value && !value.includes(":")) {
                      onChange({
                        ...data,
                        hostOverride: { Address: `${value}:80` },
                      });
                    }
                  }}
                  placeholder="192.168.1.100"
                />
              </div>
            )}

            {data.hostOverride?.Hostname && (
              <div className="ml-6 space-y-3">
                <Label htmlFor="host-hostname">Hostname</Label>
                <Input
                  id="host-hostname"
                  value={data.hostOverride.Hostname[0] || ""}
                  onChange={(e) =>
                    onChange({
                      ...data,
                      hostOverride: {
                        Hostname: [e.target.value, data.hostOverride.Hostname[1]],
                      },
                    })
                  }
                  placeholder="api.example.com"
                />
                <Label htmlFor="host-port">Port</Label>
                <Input
                  id="host-port"
                  type="number"
                  min="1"
                  max="65535"
                  value={data.hostOverride.Hostname[1] || 443}
                  onChange={(e) =>
                    onChange({
                      ...data,
                      hostOverride: {
                        Hostname: [data.hostOverride.Hostname[0], parseInt(e.target.value) || 443],
                      },
                    })
                  }
                  placeholder="443"
                />
              </div>
            )}
          </div>
        </div>
      );

    default:
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Label htmlFor="rawData">Raw Configuration (JSON)</Label>
            <Textarea
              id="rawData"
              value={data.__rawInput || JSON.stringify(data, null, 2)}
              onChange={(e) => {
                // Store the raw input to allow free typing
                const value = e.target.value;
                try {
                  const parsed = JSON.parse(value);
                  // Valid JSON, store the parsed data without raw input
                  onChange(parsed);
                } catch {
                  // Invalid JSON, store raw input to preserve typing
                  onChange({ ...data, __rawInput: value });
                }
              }}
              onBlur={(e) => {
                // Final validation on blur
                const value = e.target.value;
                try {
                  const parsed = JSON.parse(value);
                  onChange(parsed);
                } catch {
                  // If still invalid, revert to valid JSON
                  const cleanData = { ...data };
                  delete cleanData.__rawInput;
                  onChange(cleanData);
                }
              }}
              placeholder="{}"
              rows={8}
              className="font-mono"
            />
          </div>
        </div>
      );
  }
}

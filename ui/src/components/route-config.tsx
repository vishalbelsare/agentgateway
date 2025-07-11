"use client";

import { useState, useEffect } from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
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
import {
  Plus,
  Route,
  Network,
  ChevronDown,
  ChevronRight,
  Trash2,
  Edit,
  Globe,
  Server,
  Loader2,
} from "lucide-react";
import { Route as RouteType, TcpRoute, Listener, Bind, PathMatch, Match } from "@/lib/types";
import { fetchBinds, updateConfig, fetchConfig } from "@/lib/api";
import { useServer } from "@/lib/server-context";
import { toast } from "sonner";

interface RouteWithContext {
  route: RouteType;
  listener: Listener;
  bind: Bind;
  routeIndex: number;
}

interface TcpRouteWithContext {
  route: TcpRoute;
  listener: Listener;
  bind: Bind;
  routeIndex: number;
}

interface CombinedRouteWithContext {
  type: "http" | "tcp";
  route: RouteType | TcpRoute;
  listener: Listener;
  bind: Bind;
  routeIndex: number;
}

export function RouteConfig() {
  const { refreshListeners } = useServer();

  // Helper function to determine if a listener protocol supports TCP routes
  const isTcpListener = (listener: Listener) => {
    const protocol = listener.protocol || "HTTP";
    return protocol === "TCP" || protocol === "TLS";
  };
  const [binds, setBinds] = useState<Bind[]>([]);
  const [expandedBinds, setExpandedBinds] = useState<Set<number>>(new Set());
  const [routes, setRoutes] = useState<RouteWithContext[]>([]);
  const [tcpRoutes, setTcpRoutes] = useState<TcpRouteWithContext[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Dialog states
  const [isAddRouteDialogOpen, setIsAddRouteDialogOpen] = useState(false);
  const [isEditRouteDialogOpen, setIsEditRouteDialogOpen] = useState(false);
  const [editingRoute, setEditingRoute] = useState<RouteWithContext | null>(null);
  const [isEditTcpRouteDialogOpen, setIsEditTcpRouteDialogOpen] = useState(false);
  const [editingTcpRoute, setEditingTcpRoute] = useState<TcpRouteWithContext | null>(null);
  const [selectedListener, setSelectedListener] = useState<{
    listener: Listener;
    bind: Bind;
  } | null>(null);
  const [selectedRouteType, setSelectedRouteType] = useState<"http" | "tcp">("http");

  // Form states
  const [routeForm, setRouteForm] = useState({
    name: "",
    ruleName: "",
    hostnames: "",
    pathPrefix: "/",
    pathType: "pathPrefix" as keyof PathMatch,
    headers: "",
    methods: "",
    queryParams: "",
  });

  // TCP route form states
  const [tcpRouteForm, setTcpRouteForm] = useState({
    name: "",
    ruleName: "",
    hostnames: "",
  });

  const loadRoutes = async () => {
    setIsLoading(true);
    try {
      const fetchedBinds = await fetchBinds();
      setBinds(fetchedBinds);

      // Extract all routes with context
      const allRoutes: RouteWithContext[] = [];
      const allTcpRoutes: TcpRouteWithContext[] = [];

      fetchedBinds.forEach((bind) => {
        bind.listeners.forEach((listener) => {
          listener.routes?.forEach((route, routeIndex) => {
            allRoutes.push({ route, listener, bind, routeIndex });
          });

          listener.tcpRoutes?.forEach((tcpRoute, routeIndex) => {
            allTcpRoutes.push({ route: tcpRoute, listener, bind, routeIndex });
          });
        });
      });

      setRoutes(allRoutes);
      setTcpRoutes(allTcpRoutes);

      // Auto-expand binds with routes
      const bindsWithRoutes = new Set<number>();
      allRoutes.forEach(({ bind }) => bindsWithRoutes.add(bind.port));
      allTcpRoutes.forEach(({ bind }) => bindsWithRoutes.add(bind.port));
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

  const resetRouteForm = () => {
    setRouteForm({
      name: "",
      ruleName: "",
      hostnames: "",
      pathPrefix: "/",
      pathType: "pathPrefix",
      headers: "",
      methods: "",
      queryParams: "",
    });
    setTcpRouteForm({
      name: "",
      ruleName: "",
      hostnames: "",
    });
    setSelectedListener(null);
    setSelectedRouteType("http");
  };

  const parseStringArray = (str: string): string[] => {
    return str
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  };

  const buildMatch = (): Match => {
    const match: Match = {
      path:
        routeForm.pathType === "regex"
          ? { regex: [routeForm.pathPrefix, 0] }
          : { [routeForm.pathType]: routeForm.pathPrefix },
    };

    // Add headers if provided
    if (routeForm.headers) {
      const headerPairs = parseStringArray(routeForm.headers);
      match.headers = headerPairs.map((pair) => {
        const [name, value] = pair.split(":").map((s) => s.trim());
        return {
          name,
          value: { exact: value || "" },
        };
      });
    }

    // Add methods if provided
    if (routeForm.methods) {
      match.method = { method: routeForm.methods.trim() };
    }

    // Add query params if provided
    if (routeForm.queryParams) {
      const queryPairs = parseStringArray(routeForm.queryParams);
      match.query = queryPairs.map((pair) => {
        const [name, value] = pair.split("=").map((s) => s.trim());
        return {
          name,
          value: { exact: value || "" },
        };
      });
    }

    return match;
  };

  const handleAddRoute = async () => {
    if (!selectedListener) {
      toast.error("Please select a listener");
      return;
    }

    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find the specific bind and listener
      const bind = config.binds.find((b) => b.port === selectedListener.bind.port);
      const listener = bind?.listeners.find((l) => l.name === selectedListener.listener.name);

      if (!bind || !listener) {
        throw new Error("Could not find bind or listener");
      }

      if (isTcpListener(selectedListener.listener)) {
        // Ensure tcpRoutes array exists
        if (!listener.tcpRoutes) {
          listener.tcpRoutes = [];
        }

        // Create new TCP route
        const newTcpRoute: TcpRoute = {
          hostnames: parseStringArray(tcpRouteForm.hostnames),
          backends: [],
        };

        if (tcpRouteForm.name) newTcpRoute.name = tcpRouteForm.name;
        if (tcpRouteForm.ruleName) newTcpRoute.ruleName = tcpRouteForm.ruleName;

        listener.tcpRoutes.push(newTcpRoute);
      } else {
        // Ensure routes array exists
        if (!listener.routes) {
          listener.routes = [];
        }

        // Create new HTTP route
        const newRoute: RouteType = {
          hostnames: parseStringArray(routeForm.hostnames),
          matches: [buildMatch()],
          backends: [],
        };

        if (routeForm.name) newRoute.name = routeForm.name;
        if (routeForm.ruleName) newRoute.ruleName = routeForm.ruleName;

        listener.routes.push(newRoute);
      }

      await updateConfig(config);
      await loadRoutes();
      await refreshListeners();

      resetRouteForm();
      setIsAddRouteDialogOpen(false);
      const routeType = isTcpListener(selectedListener.listener) ? "TCP" : "HTTP";
      toast.success(`${routeType} route added successfully`);
    } catch (err) {
      console.error("Error adding route:", err);
      toast.error(err instanceof Error ? err.message : "Failed to add route");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteRoute = async (routeContext: RouteWithContext) => {
    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find and remove the route
      const bind = config.binds.find((b) => b.port === routeContext.bind.port);
      const listener = bind?.listeners.find((l) => l.name === routeContext.listener.name);

      if (listener?.routes) {
        listener.routes.splice(routeContext.routeIndex, 1);
      }

      await updateConfig(config);
      await loadRoutes();
      await refreshListeners();

      toast.success("Route deleted successfully");
    } catch (err) {
      console.error("Error deleting route:", err);
      toast.error("Failed to delete route");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteTcpRoute = async (tcpRouteContext: TcpRouteWithContext) => {
    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find and remove the TCP route
      const bind = config.binds.find((b) => b.port === tcpRouteContext.bind.port);
      const listener = bind?.listeners.find((l) => l.name === tcpRouteContext.listener.name);

      if (listener?.tcpRoutes) {
        listener.tcpRoutes.splice(tcpRouteContext.routeIndex, 1);
      }

      await updateConfig(config);
      await loadRoutes();
      await refreshListeners();

      toast.success("TCP route deleted successfully");
    } catch (err) {
      console.error("Error deleting TCP route:", err);
      toast.error("Failed to delete TCP route");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleEditRoute = async () => {
    if (!editingRoute) {
      toast.error("No route selected for editing");
      return;
    }

    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find the specific bind, listener, and route
      const bind = config.binds.find((b) => b.port === editingRoute.bind.port);
      const listener = bind?.listeners.find((l) => l.name === editingRoute.listener.name);

      if (!bind || !listener || !listener.routes) {
        throw new Error("Could not find bind, listener, or routes");
      }

      // Update the route
      const updatedRoute: RouteType = {
        hostnames: parseStringArray(routeForm.hostnames),
        matches: [buildMatch()],
        backends: editingRoute.route.backends, // Keep existing backends
      };

      if (routeForm.name) updatedRoute.name = routeForm.name;
      if (routeForm.ruleName) updatedRoute.ruleName = routeForm.ruleName;

      // Replace the route at the specific index
      listener.routes[editingRoute.routeIndex] = updatedRoute;

      await updateConfig(config);
      await loadRoutes();
      await refreshListeners();

      resetRouteForm();
      setEditingRoute(null);
      setIsEditRouteDialogOpen(false);
      toast.success("Route updated successfully");
    } catch (err) {
      console.error("Error updating route:", err);
      toast.error(err instanceof Error ? err.message : "Failed to update route");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleEditTcpRoute = async () => {
    if (!editingTcpRoute) {
      toast.error("No TCP route selected for editing");
      return;
    }

    setIsSubmitting(true);
    try {
      const config = await fetchConfig();

      // Find the specific bind, listener, and TCP route
      const bind = config.binds.find((b) => b.port === editingTcpRoute.bind.port);
      const listener = bind?.listeners.find((l) => l.name === editingTcpRoute.listener.name);

      if (!bind || !listener || !listener.tcpRoutes) {
        throw new Error("Could not find bind, listener, or TCP routes");
      }

      // Update the TCP route
      const updatedTcpRoute: TcpRoute = {
        hostnames: parseStringArray(tcpRouteForm.hostnames),
        backends: editingTcpRoute.route.backends, // Keep existing backends
      };

      if (tcpRouteForm.name) updatedTcpRoute.name = tcpRouteForm.name;
      if (tcpRouteForm.ruleName) updatedTcpRoute.ruleName = tcpRouteForm.ruleName;

      // Replace the TCP route at the specific index
      listener.tcpRoutes[editingTcpRoute.routeIndex] = updatedTcpRoute;

      await updateConfig(config);
      await loadRoutes();
      await refreshListeners();

      resetRouteForm();
      setEditingTcpRoute(null);
      setIsEditTcpRouteDialogOpen(false);
      toast.success("TCP route updated successfully");
    } catch (err) {
      console.error("Error updating TCP route:", err);
      toast.error(err instanceof Error ? err.message : "Failed to update TCP route");
    } finally {
      setIsSubmitting(false);
    }
  };

  const populateEditForm = (routeContext: RouteWithContext) => {
    const route = routeContext.route;
    const firstMatch = route.matches?.[0];

    setRouteForm({
      name: route.name || "",
      ruleName: route.ruleName || "",
      hostnames: route.hostnames?.join(", ") || "",
      pathPrefix:
        firstMatch?.path.pathPrefix || firstMatch?.path.exact || firstMatch?.path.regex?.[0] || "/",
      pathType: firstMatch?.path.pathPrefix
        ? "pathPrefix"
        : firstMatch?.path.exact
          ? "exact"
          : firstMatch?.path.regex
            ? "regex"
            : "pathPrefix",
      headers: firstMatch?.headers?.map((h) => `${h.name}:${h.value.exact || ""}`).join(", ") || "",
      methods: firstMatch?.method?.method || "",
      queryParams:
        firstMatch?.query?.map((q) => `${q.name}=${q.value.exact || ""}`).join(", ") || "",
    });
  };

  const populateTcpEditForm = (tcpRouteContext: TcpRouteWithContext) => {
    const tcpRoute = tcpRouteContext.route;

    setTcpRouteForm({
      name: tcpRoute.name || "",
      ruleName: tcpRoute.ruleName || "",
      hostnames: tcpRoute.hostnames?.join(", ") || "",
    });
  };

  const getAvailableListeners = () => {
    const listeners: { listener: Listener; bind: Bind }[] = [];
    binds.forEach((bind) => {
      bind.listeners.forEach((listener) => {
        listeners.push({ listener, bind });
      });
    });
    return listeners;
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

  const getTcpRoutesByBind = () => {
    const tcpRoutesByBind = new Map<number, TcpRouteWithContext[]>();
    tcpRoutes.forEach((tcpRouteContext) => {
      const port = tcpRouteContext.bind.port;
      if (!tcpRoutesByBind.has(port)) {
        tcpRoutesByBind.set(port, []);
      }
      tcpRoutesByBind.get(port)!.push(tcpRouteContext);
    });
    return tcpRoutesByBind;
  };

  const getAllRoutesByBind = () => {
    const allRoutesByBind = new Map<number, CombinedRouteWithContext[]>();

    // Get all unique ports
    const allPorts = new Set<number>();
    routes.forEach((r) => allPorts.add(r.bind.port));
    tcpRoutes.forEach((r) => allPorts.add(r.bind.port));

    // Initialize each port with empty arrays
    allPorts.forEach((port) => {
      allRoutesByBind.set(port, []);
    });

    // Populate with HTTP routes
    routes.forEach((routeContext) => {
      const port = routeContext.bind.port;
      allRoutesByBind.get(port)!.push({
        type: "http",
        route: routeContext.route,
        listener: routeContext.listener,
        bind: routeContext.bind,
        routeIndex: routeContext.routeIndex,
      });
    });

    // Populate with TCP routes
    tcpRoutes.forEach((tcpRouteContext) => {
      const port = tcpRouteContext.bind.port;
      allRoutesByBind.get(port)!.push({
        type: "tcp",
        route: tcpRouteContext.route,
        listener: tcpRouteContext.listener,
        bind: tcpRouteContext.bind,
        routeIndex: tcpRouteContext.routeIndex,
      });
    });

    return allRoutesByBind;
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
      <div className="flex justify-end">
        <Button onClick={() => setIsAddRouteDialogOpen(true)}>
          <Plus className="mr-2 h-4 w-4" />
          Add Route
        </Button>
      </div>

      {routes.length === 0 && tcpRoutes.length === 0 ? (
        <div className="text-center py-12 border rounded-md bg-muted/20">
          <Route className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
          <p className="text-muted-foreground">No routes configured. Add a route to get started.</p>
        </div>
      ) : (
        <div className="space-y-4">
          {Array.from(getAllRoutesByBind().entries()).map(([port, combinedRoutes]) => {
            if (combinedRoutes.length === 0) return null;

            const httpCount = combinedRoutes.filter((r) => r.type === "http").length;
            const tcpCount = combinedRoutes.filter((r) => r.type === "tcp").length;

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
                              {httpCount > 0 && (
                                <div className="flex items-center space-x-1">
                                  <Globe className="h-3 w-3 text-green-500" />
                                  <span>{httpCount} HTTP</span>
                                </div>
                              )}
                              {tcpCount > 0 && (
                                <div className="flex items-center space-x-1">
                                  <Server className="h-3 w-3 text-blue-500" />
                                  <span>{tcpCount} TCP</span>
                                </div>
                              )}
                            </div>
                          </div>
                        </div>
                        <Badge variant="secondary">{combinedRoutes.length} routes</Badge>
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
                            <TableHead>Hostnames</TableHead>
                            <TableHead>Path</TableHead>
                            <TableHead>Backends</TableHead>
                            <TableHead className="text-right">Actions</TableHead>
                          </TableRow>
                        </TableHeader>
                        <TableBody>
                          {combinedRoutes.map((combinedRoute, index) => (
                            <TableRow key={`${combinedRoute.type}-${index}`}>
                              <TableCell className="font-medium">
                                {combinedRoute.route.name ||
                                  `${combinedRoute.type.toUpperCase()} Route ${index + 1}`}
                              </TableCell>
                              <TableCell>
                                <Badge
                                  variant={combinedRoute.type === "http" ? "default" : "secondary"}
                                  className={
                                    combinedRoute.type === "http"
                                      ? "bg-green-500 hover:bg-green-600"
                                      : "bg-blue-500 hover:bg-blue-600 text-white"
                                  }
                                >
                                  {combinedRoute.type === "http" ? (
                                    <Globe className="mr-1 h-3 w-3" />
                                  ) : (
                                    <Server className="mr-1 h-3 w-3" />
                                  )}
                                  {combinedRoute.type.toUpperCase()}
                                </Badge>
                              </TableCell>
                              <TableCell>
                                <Badge variant="outline">
                                  {combinedRoute.listener.name || "unnamed"}
                                </Badge>
                              </TableCell>
                              <TableCell>
                                {combinedRoute.route.hostnames &&
                                combinedRoute.route.hostnames.length > 0 ? (
                                  <div className="flex flex-wrap gap-1">
                                    {combinedRoute.route.hostnames.map((hostname, i) => (
                                      <Badge key={i} variant="secondary" className="text-xs">
                                        {hostname}
                                      </Badge>
                                    ))}
                                  </div>
                                ) : (
                                  <span className="text-muted-foreground">*</span>
                                )}
                              </TableCell>
                              <TableCell>
                                {combinedRoute.type === "http" ? (
                                  (combinedRoute.route as RouteType).matches &&
                                  (combinedRoute.route as RouteType).matches.length > 0 ? (
                                    (combinedRoute.route as RouteType).matches.map((match, i) => (
                                      <Badge key={i} variant="outline" className="mr-1">
                                        {match.path.exact
                                          ? `= ${match.path.exact}`
                                          : match.path.pathPrefix
                                            ? `${match.path.pathPrefix}*`
                                            : match.path.regex
                                              ? `~ ${match.path.regex[0]}`
                                              : "/"}
                                      </Badge>
                                    ))
                                  ) : (
                                    <span className="text-muted-foreground text-sm">*</span>
                                  )
                                ) : (
                                  <span className="text-muted-foreground text-sm">N/A</span>
                                )}
                              </TableCell>
                              <TableCell>
                                <Badge variant="outline">
                                  {combinedRoute.route.backends?.length || 0} backend
                                  {(combinedRoute.route.backends?.length || 0) !== 1 ? "s" : ""}
                                </Badge>
                              </TableCell>
                              <TableCell className="text-right">
                                <div className="flex justify-end space-x-2">
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    onClick={() => {
                                      if (combinedRoute.type === "http") {
                                        const httpRouteContext: RouteWithContext = {
                                          route: combinedRoute.route as RouteType,
                                          listener: combinedRoute.listener,
                                          bind: combinedRoute.bind,
                                          routeIndex: combinedRoute.routeIndex,
                                        };
                                        setEditingRoute(httpRouteContext);
                                        populateEditForm(httpRouteContext);
                                        setIsEditRouteDialogOpen(true);
                                      } else {
                                        const tcpRouteContext: TcpRouteWithContext = {
                                          route: combinedRoute.route as TcpRoute,
                                          listener: combinedRoute.listener,
                                          bind: combinedRoute.bind,
                                          routeIndex: combinedRoute.routeIndex,
                                        };
                                        setEditingTcpRoute(tcpRouteContext);
                                        populateTcpEditForm(tcpRouteContext);
                                        setIsEditTcpRouteDialogOpen(true);
                                      }
                                    }}
                                  >
                                    <Edit className="h-4 w-4" />
                                  </Button>
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    onClick={() => {
                                      if (combinedRoute.type === "http") {
                                        const httpRouteContext: RouteWithContext = {
                                          route: combinedRoute.route as RouteType,
                                          listener: combinedRoute.listener,
                                          bind: combinedRoute.bind,
                                          routeIndex: combinedRoute.routeIndex,
                                        };
                                        handleDeleteRoute(httpRouteContext);
                                      } else {
                                        const tcpRouteContext: TcpRouteWithContext = {
                                          route: combinedRoute.route as TcpRoute,
                                          listener: combinedRoute.listener,
                                          bind: combinedRoute.bind,
                                          routeIndex: combinedRoute.routeIndex,
                                        };
                                        handleDeleteTcpRoute(tcpRouteContext);
                                      }
                                    }}
                                    className="text-destructive hover:text-destructive"
                                  >
                                    <Trash2 className="h-4 w-4" />
                                  </Button>
                                </div>
                              </TableCell>
                            </TableRow>
                          ))}
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

      {/* Add Route Dialog */}
      <Dialog open={isAddRouteDialogOpen} onOpenChange={setIsAddRouteDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Add Route</DialogTitle>
            <DialogDescription>
              Create a new HTTP or TCP route with matching conditions and routing rules.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {/* Route Type Selection - Auto-determined by listener protocol */}
            {selectedListener && (
              <div className="space-y-2">
                <Label>Route Type</Label>
                <div className="p-3 bg-muted rounded-md">
                  <div className="flex items-center space-x-2">
                    {isTcpListener(selectedListener.listener) ? (
                      <>
                        <Server className="h-4 w-4 text-blue-500" />
                        <span className="font-medium">TCP Route</span>
                      </>
                    ) : (
                      <>
                        <Globe className="h-4 w-4 text-green-500" />
                        <span className="font-medium">HTTP Route</span>
                      </>
                    )}
                  </div>
                  <p className="text-xs text-muted-foreground mt-1">
                    Route type automatically determined by listener protocol:{" "}
                    {selectedListener.listener.protocol || "HTTP"}
                  </p>
                </div>
              </div>
            )}

            {/* Common fields for both HTTP and TCP */}
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="route-name">Route Name (optional)</Label>
                <Input
                  id="route-name"
                  value={
                    selectedListener && isTcpListener(selectedListener.listener)
                      ? tcpRouteForm.name
                      : routeForm.name
                  }
                  onChange={(e) => {
                    if (selectedListener && isTcpListener(selectedListener.listener)) {
                      setTcpRouteForm((prev) => ({ ...prev, name: e.target.value }));
                    } else {
                      setRouteForm((prev) => ({ ...prev, name: e.target.value }));
                    }
                  }}
                  placeholder="my-route"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="rule-name">Rule Name (optional)</Label>
                <Input
                  id="rule-name"
                  value={
                    selectedListener && isTcpListener(selectedListener.listener)
                      ? tcpRouteForm.ruleName
                      : routeForm.ruleName
                  }
                  onChange={(e) => {
                    if (selectedListener && isTcpListener(selectedListener.listener)) {
                      setTcpRouteForm((prev) => ({ ...prev, ruleName: e.target.value }));
                    } else {
                      setRouteForm((prev) => ({ ...prev, ruleName: e.target.value }));
                    }
                  }}
                  placeholder="rule-1"
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="listener-select">Target Listener *</Label>
              <div className="grid gap-2">
                {getAvailableListeners().map((item, index) => (
                  <Button
                    key={index}
                    variant={
                      selectedListener?.listener.name === item.listener.name ? "default" : "outline"
                    }
                    onClick={() => {
                      setSelectedListener(item);
                      // Auto-set route type based on listener protocol
                      setSelectedRouteType(isTcpListener(item.listener) ? "tcp" : "http");
                    }}
                    className="justify-start"
                  >
                    <Network className="mr-2 h-4 w-4" />
                    {item.listener.name || `Unnamed`} (Port {item.bind.port}) -{" "}
                    {item.listener.protocol || "HTTP"}
                  </Button>
                ))}
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="hostnames">Hostnames (comma-separated)</Label>
              <Input
                id="hostnames"
                value={
                  selectedListener && isTcpListener(selectedListener.listener)
                    ? tcpRouteForm.hostnames
                    : routeForm.hostnames
                }
                onChange={(e) => {
                  if (selectedListener && isTcpListener(selectedListener.listener)) {
                    setTcpRouteForm((prev) => ({ ...prev, hostnames: e.target.value }));
                  } else {
                    setRouteForm((prev) => ({ ...prev, hostnames: e.target.value }));
                  }
                }}
                placeholder="example.com, *.example.com"
              />
              <p className="text-xs text-muted-foreground">Leave empty to match all hostnames</p>
            </div>

            {/* HTTP-specific fields */}
            {selectedListener && !isTcpListener(selectedListener.listener) && (
              <>
                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="path-type">Path Match Type</Label>
                    <Select
                      value={routeForm.pathType}
                      onValueChange={(value) =>
                        setRouteForm((prev) => ({ ...prev, pathType: value as keyof PathMatch }))
                      }
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="pathPrefix">Path Prefix</SelectItem>
                        <SelectItem value="exact">Exact Path</SelectItem>
                        <SelectItem value="regex">Regex Pattern</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="path-value">
                      {routeForm.pathType === "pathPrefix"
                        ? "Path Prefix"
                        : routeForm.pathType === "exact"
                          ? "Exact Path"
                          : "Regex Pattern"}
                    </Label>
                    <Input
                      id="path-value"
                      value={routeForm.pathPrefix}
                      onChange={(e) =>
                        setRouteForm((prev) => ({ ...prev, pathPrefix: e.target.value }))
                      }
                      placeholder={routeForm.pathType === "regex" ? "^/api/.*" : "/"}
                    />
                  </div>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="headers">Headers (optional)</Label>
                  <Input
                    id="headers"
                    value={routeForm.headers}
                    onChange={(e) => setRouteForm((prev) => ({ ...prev, headers: e.target.value }))}
                    placeholder="Authorization: Bearer, Content-Type: application/json"
                  />
                  <p className="text-xs text-muted-foreground">Format: name:value, name:value</p>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor="methods">HTTP Methods (optional)</Label>
                    <Input
                      id="methods"
                      value={routeForm.methods}
                      onChange={(e) =>
                        setRouteForm((prev) => ({ ...prev, methods: e.target.value }))
                      }
                      placeholder="GET, POST, PUT"
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor="query-params">Query Parameters (optional)</Label>
                    <Input
                      id="query-params"
                      value={routeForm.queryParams}
                      onChange={(e) =>
                        setRouteForm((prev) => ({ ...prev, queryParams: e.target.value }))
                      }
                      placeholder="version=v1, type=json"
                    />
                  </div>
                </div>
              </>
            )}

            {/* TCP-specific fields */}
            {selectedListener && isTcpListener(selectedListener.listener) && (
              <div className="p-4 bg-muted/50 rounded-lg">
                <div className="flex items-center space-x-2 mb-2">
                  <Server className="h-4 w-4 text-muted-foreground" />
                  <span className="text-sm font-medium">TCP Route Configuration</span>
                </div>
                <p className="text-sm text-muted-foreground">
                  TCP routes are simpler than HTTP routes. They only support hostname-based routing
                  and don&apos;t have path matching, headers, or query parameters.
                </p>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setIsAddRouteDialogOpen(false);
                resetRouteForm();
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleAddRoute} disabled={isSubmitting || !selectedListener}>
              {isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Add{" "}
              {selectedListener
                ? isTcpListener(selectedListener.listener)
                  ? "TCP"
                  : "HTTP"
                : "Route"}{" "}
              Route
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit Route Dialog */}
      <Dialog open={isEditRouteDialogOpen} onOpenChange={setIsEditRouteDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Edit HTTP Route</DialogTitle>
            <DialogDescription>
              Update the route configuration and matching conditions.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="edit-route-name">Route Name (optional)</Label>
                <Input
                  id="edit-route-name"
                  value={routeForm.name}
                  onChange={(e) => setRouteForm((prev) => ({ ...prev, name: e.target.value }))}
                  placeholder="my-route"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="edit-rule-name">Rule Name (optional)</Label>
                <Input
                  id="edit-rule-name"
                  value={routeForm.ruleName}
                  onChange={(e) => setRouteForm((prev) => ({ ...prev, ruleName: e.target.value }))}
                  placeholder="rule-1"
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label>Current Listener</Label>
              <div className="p-3 bg-muted rounded-md">
                <div className="flex items-center space-x-2">
                  <Network className="h-4 w-4" />
                  <span className="font-medium">
                    {editingRoute?.listener.name || "Unnamed"} (Port {editingRoute?.bind.port})
                  </span>
                </div>
                <p className="text-xs text-muted-foreground mt-1">
                  Listener cannot be changed when editing. Delete and recreate the route to move it.
                </p>
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="edit-hostnames">Hostnames (comma-separated)</Label>
              <Input
                id="edit-hostnames"
                value={routeForm.hostnames}
                onChange={(e) => setRouteForm((prev) => ({ ...prev, hostnames: e.target.value }))}
                placeholder="example.com, *.example.com"
              />
              <p className="text-xs text-muted-foreground">Leave empty to match all hostnames</p>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="edit-path-type">Path Match Type</Label>
                <Select
                  value={routeForm.pathType}
                  onValueChange={(value) =>
                    setRouteForm((prev) => ({ ...prev, pathType: value as keyof PathMatch }))
                  }
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="pathPrefix">Path Prefix</SelectItem>
                    <SelectItem value="exact">Exact Path</SelectItem>
                    <SelectItem value="regex">Regex Pattern</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="edit-path-value">
                  {routeForm.pathType === "pathPrefix"
                    ? "Path Prefix"
                    : routeForm.pathType === "exact"
                      ? "Exact Path"
                      : "Regex Pattern"}
                </Label>
                <Input
                  id="edit-path-value"
                  value={routeForm.pathPrefix}
                  onChange={(e) =>
                    setRouteForm((prev) => ({ ...prev, pathPrefix: e.target.value }))
                  }
                  placeholder={routeForm.pathType === "regex" ? "^/api/.*" : "/"}
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="edit-headers">Headers (optional)</Label>
              <Input
                id="edit-headers"
                value={routeForm.headers}
                onChange={(e) => setRouteForm((prev) => ({ ...prev, headers: e.target.value }))}
                placeholder="Authorization: Bearer, Content-Type: application/json"
              />
              <p className="text-xs text-muted-foreground">Format: name:value, name:value</p>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="edit-methods">HTTP Methods (optional)</Label>
                <Input
                  id="edit-methods"
                  value={routeForm.methods}
                  onChange={(e) => setRouteForm((prev) => ({ ...prev, methods: e.target.value }))}
                  placeholder="GET, POST, PUT"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="edit-query-params">Query Parameters (optional)</Label>
                <Input
                  id="edit-query-params"
                  value={routeForm.queryParams}
                  onChange={(e) =>
                    setRouteForm((prev) => ({ ...prev, queryParams: e.target.value }))
                  }
                  placeholder="version=v1, type=json"
                />
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setIsEditRouteDialogOpen(false);
                setEditingRoute(null);
                resetRouteForm();
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleEditRoute} disabled={isSubmitting}>
              {isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Update Route
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit TCP Route Dialog */}
      <Dialog open={isEditTcpRouteDialogOpen} onOpenChange={setIsEditTcpRouteDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Edit TCP Route</DialogTitle>
            <DialogDescription>Update the TCP route configuration.</DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="edit-tcp-route-name">Route Name (optional)</Label>
                <Input
                  id="edit-tcp-route-name"
                  value={tcpRouteForm.name}
                  onChange={(e) => setTcpRouteForm((prev) => ({ ...prev, name: e.target.value }))}
                  placeholder="my-tcp-route"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="edit-tcp-rule-name">Rule Name (optional)</Label>
                <Input
                  id="edit-tcp-rule-name"
                  value={tcpRouteForm.ruleName}
                  onChange={(e) =>
                    setTcpRouteForm((prev) => ({ ...prev, ruleName: e.target.value }))
                  }
                  placeholder="tcp-rule-1"
                />
              </div>
            </div>

            <div className="space-y-2">
              <Label>Current Listener</Label>
              <div className="p-3 bg-muted rounded-md">
                <div className="flex items-center space-x-2">
                  <Network className="h-4 w-4" />
                  <span className="font-medium">
                    {editingTcpRoute?.listener.name || "Unnamed"} (Port {editingTcpRoute?.bind.port}
                    )
                  </span>
                </div>
                <p className="text-xs text-muted-foreground mt-1">
                  Listener cannot be changed when editing. Delete and recreate the route to move it.
                </p>
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="edit-tcp-hostnames">Hostnames (comma-separated)</Label>
              <Input
                id="edit-tcp-hostnames"
                value={tcpRouteForm.hostnames}
                onChange={(e) =>
                  setTcpRouteForm((prev) => ({ ...prev, hostnames: e.target.value }))
                }
                placeholder="example.com, *.example.com"
              />
              <p className="text-xs text-muted-foreground">Leave empty to match all hostnames</p>
            </div>

            <div className="p-4 bg-muted/50 rounded-lg">
              <div className="flex items-center space-x-2 mb-2">
                <Server className="h-4 w-4 text-muted-foreground" />
                <span className="text-sm font-medium">TCP Route Configuration</span>
              </div>
              <p className="text-sm text-muted-foreground">
                TCP routes are simpler than HTTP routes. They only support hostname-based routing
                and don&apos;t have path matching, headers, or query parameters.
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setIsEditTcpRouteDialogOpen(false);
                setEditingTcpRoute(null);
                resetRouteForm();
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleEditTcpRoute} disabled={isSubmitting}>
              {isSubmitting && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              Update TCP Route
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

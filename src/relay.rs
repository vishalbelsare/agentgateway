use crate::rbac;
use crate::state::State;
use rmcp::{
	Error as McpError, RoleServer, ServerHandler, model::CallToolRequestParam, model::Tool, model::*,
	service::RequestContext,
};
use std::borrow::Cow;
use std::sync::Arc;
#[derive(Clone)]
pub struct Relay {
	state: Arc<State>,
	id: rbac::Identity,
}

impl Relay {
	pub fn new(state: Arc<State>, id: rbac::Identity) -> Self {
		Self { state, id }
	}
}

// TODO: lists and gets can be macros
impl ServerHandler for Relay {
	fn get_info(&self) -> ServerInfo {
		ServerInfo {
      protocol_version: ProtocolVersion::V_2024_11_05,
      capabilities: ServerCapabilities {
          experimental: None,
          logging: None,
          prompts: Some(PromptsCapability::default()),
          resources: Some(ResourcesCapability::default()),
          tools: Some(ToolsCapability {
              list_changed: None,
          }),
      },
      server_info: Implementation::from_build_env(),
			instructions: Some(
				"This server provides a counter tool that can increment and decrement values. The counter starts at 0 and can be modified using the 'increment' and 'decrement' tools. Use 'get_value' to check the current count.".to_string(),
			),
		}
	}

	async fn list_resources(
		&self,
		request: PaginatedRequestParam,
		_context: RequestContext<RoleServer>,
	) -> std::result::Result<ListResourcesResult, McpError> {
		let all = self.state.targets.iter().await.map(|(_name, svc)| async {
			let result = svc
				.as_ref()
				.read()
				.await
				.list_resources(request.clone())
				.await
				.unwrap();
			result.resources
		});

		Ok(ListResourcesResult {
			resources: futures::future::join_all(all)
				.await
				.into_iter()
				.flatten()
				.collect(),
			next_cursor: None,
		})
	}

	async fn read_resource(
		&self,
		request: ReadResourceRequestParam,
		_context: RequestContext<RoleServer>,
	) -> std::result::Result<ReadResourceResult, McpError> {
		let all = self.state.targets.iter().await.map(|(_name, svc)| async {
			let result = svc
				.as_ref()
				.read()
				.await
				.read_resource(request.clone())
				.await
				.unwrap();
			result.contents
		});

		Ok(ReadResourceResult {
			contents: futures::future::join_all(all)
				.await
				.into_iter()
				.flatten()
				.collect(),
		})
	}

	async fn list_resource_templates(
		&self,
		request: PaginatedRequestParam,
		_context: RequestContext<RoleServer>,
	) -> std::result::Result<ListResourceTemplatesResult, McpError> {
		let all = self.state.targets.iter().await.map(|(_name, svc)| async {
			let result = svc
				.as_ref()
				.read()
				.await
				.list_resource_templates(request.clone())
				.await
				.unwrap();
			result.resource_templates
		});

		Ok(ListResourceTemplatesResult {
			resource_templates: futures::future::join_all(all)
				.await
				.into_iter()
				.flatten()
				.collect(),
			next_cursor: None,
		})
	}

	async fn list_prompts(
		&self,
		request: PaginatedRequestParam,
		_context: RequestContext<RoleServer>,
	) -> std::result::Result<ListPromptsResult, McpError> {
		let all = self.state.targets.iter().await.map(|(_name, svc)| async {
			let result = svc
				.as_ref()
				.read()
				.await
				.list_prompts(request.clone())
				.await
				.unwrap();
			result.prompts
		});

		Ok(ListPromptsResult {
			prompts: futures::future::join_all(all)
				.await
				.into_iter()
				.flatten()
				.collect(),
			next_cursor: None,
		})
	}

	async fn get_prompt(
		&self,
		request: GetPromptRequestParam,
		_context: RequestContext<RoleServer>,
	) -> std::result::Result<GetPromptResult, McpError> {
		if !self.state.policies.validate(
			&rbac::ResourceType::Prompt {
				id: request.name.to_string(),
			},
			&self.id,
		) {
			return Err(McpError::invalid_request("not allowed", None));
		}
		let tool_name = request.name.to_string();
		let (service_name, tool) = tool_name.split_once(':').unwrap();
		let service = self.state.targets.get(service_name).await.unwrap();
		let req = GetPromptRequestParam {
			name: tool.to_string(),
			arguments: request.arguments,
		};

		let result = service.as_ref().read().await.get_prompt(req).await.unwrap();
		Ok(result)
	}

	async fn list_tools(
		&self,
		request: PaginatedRequestParam,
		_context: RequestContext<RoleServer>,
	) -> std::result::Result<ListToolsResult, McpError> {
		let mut tools = Vec::new();
		// TODO: Use iterators
		// TODO: Handle individual errors
		// TODO: Do we want to handle pagination here, or just pass it through?
		for (name, service) in self.state.targets.iter().await {
			let result = service
				.as_ref()
				.read()
				.await
				.list_tools(request.clone())
				.await
				.unwrap();
			for tool in result.tools {
				let tool_name = format!("{}:{}", name, tool.name);
				tools.push(Tool {
					name: Cow::Owned(tool_name),
					description: tool.description,
					input_schema: tool.input_schema,
				});
			}
		}
		Ok(ListToolsResult {
			tools,
			next_cursor: None,
		})
	}

	async fn call_tool(
		&self,
		request: CallToolRequestParam,
		_context: RequestContext<RoleServer>,
	) -> std::result::Result<CallToolResult, McpError> {
		if !self.state.policies.validate(
			&rbac::ResourceType::Tool {
				id: request.name.to_string(),
			},
			&self.id,
		) {
			return Err(McpError::invalid_request("not allowed", None));
		}
		let tool_name = request.name.to_string();
		let (service_name, tool) = tool_name.split_once(':').unwrap();
		let service = self.state.targets.get(service_name).await.unwrap();
		let req = CallToolRequestParam {
			name: Cow::Owned(tool.to_string()),
			arguments: request.arguments,
		};

		let result = service.as_ref().read().await.call_tool(req).await.unwrap();
		Ok(result)
	}
}

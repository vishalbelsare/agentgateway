use crate::rbac;
use crate::rbac::RbacEngine;
use rmcp::service::RunningService;
use rmcp::{
	ClientHandlerService, Error as McpError, RoleServer, ServerHandler, model::CallToolRequestParam,
	model::Tool, model::*, service::RequestContext,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
#[derive(Clone)]
pub struct Relay {
	pub rbac: RbacEngine,
	pub services: HashMap<String, Arc<Mutex<RunningService<ClientHandlerService>>>>,
}

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
            instructions: Some("This server provides a counter tool that can increment and decrement values. The counter starts at 0 and can be modified using the 'increment' and 'decrement' tools. Use 'get_value' to check the current count.".to_string()),
        }
	}

	async fn list_resources(
		&self,
		request: PaginatedRequestParam,
		context: RequestContext<RoleServer>,
	) -> std::result::Result<ListResourcesResult, McpError> {
		Ok(ListResourcesResult {
			resources: vec![],
			next_cursor: None,
		})
	}

	async fn read_resource(
		&self,
		request: ReadResourceRequestParam,
		context: RequestContext<RoleServer>,
	) -> std::result::Result<ReadResourceResult, McpError> {
		Ok(ReadResourceResult { contents: vec![] })
	}

	async fn list_resource_templates(
		&self,
		request: PaginatedRequestParam,
		context: RequestContext<RoleServer>,
	) -> std::result::Result<ListResourceTemplatesResult, McpError> {
		Ok(ListResourceTemplatesResult {
			resource_templates: vec![],
			next_cursor: None,
		})
	}

	async fn list_prompts(
		&self,
		request: PaginatedRequestParam,
		context: RequestContext<RoleServer>,
	) -> std::result::Result<ListPromptsResult, McpError> {
		Ok(ListPromptsResult {
			prompts: vec![],
			next_cursor: None,
		})
	}

	async fn get_prompt(
		&self,
		request: GetPromptRequestParam,
		context: RequestContext<RoleServer>,
	) -> std::result::Result<GetPromptResult, McpError> {
		Ok(GetPromptResult {
			description: None,
			messages: vec![],
		})
	}

	async fn list_tools(
		&self,
		request: PaginatedRequestParam,
		context: RequestContext<RoleServer>,
	) -> std::result::Result<ListToolsResult, McpError> {
		let mut tools = Vec::new();
		for (name, service) in self.services.iter() {
			let result = service
				.as_ref()
				.lock()
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
		context: RequestContext<RoleServer>,
	) -> std::result::Result<CallToolResult, McpError> {
		if !self.rbac.check(rbac::ResourceType::Tool {
			id: request.name.to_string(),
		}) {
			return Err(McpError::method_not_found::<CallToolRequestMethod>());
		}
		let tool_name = request.name.to_string();
		let (service_name, tool) = tool_name.split_once(':').unwrap();
		let service = self.services.get(service_name).unwrap();
		let req = CallToolRequestParam {
			name: Cow::Owned(tool.to_string()),
			arguments: request.arguments,
		};

		let result = service.as_ref().lock().await.call_tool(req).await.unwrap();
		Ok(result)
	}
}

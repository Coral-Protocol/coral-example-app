use std::collections::HashMap;
use coral_rs::api::generated::{Client, Error, ResponseValue};
use coral_rs::api::generated::types::{AgentGraphRequest, AgentOptionValue, AgentRegistryIdentifier, GraphAgentProvider, GraphAgentRequest, RouteException, RuntimeId, SessionIdentifier, SessionRequest};
use humantime::format_duration;
use crate::Arguments;

pub struct Session<'a> {
    arguments: &'a Arguments,
    channel_id: String,
}

impl<'a> Session<'a> {
    pub fn new(
        arguments: &'a Arguments,
        channel_id: String,
    ) -> Self {
        Self {
            arguments,
            channel_id,
        }
    }

    fn discord_graph_request(&self) -> GraphAgentRequest {
        let mut options = HashMap::from([
            ("DISCORD_API_TOKEN".to_string(), AgentOptionValue::String(self.arguments.discord_api_token.clone())),
            ("OPENROUTER_API_KEY".to_string(), AgentOptionValue::String(self.arguments.openrouter_api_key.clone())),
            ("DISCORD_THREAD_ID".to_string(), AgentOptionValue::String(self.channel_id.clone()))
        ]);

        if let Some(timeout) = self.arguments.timeout_duration_warning {
            options.insert("DISCORD_TIMEOUT_WARNING".to_string(),
                           AgentOptionValue::String(format_duration(timeout.into()).to_string()));
        }

        if let Some(timeout) = self.arguments.timeout_duration {
            options.insert("DISCORD_TIMEOUT_WARNING".to_string(),
                           AgentOptionValue::String(format_duration(timeout.into()).to_string()));
        }

        GraphAgentRequest {
            blocking: Some(true),
            coral_plugins: vec![],
            custom_tool_access: vec![],
            description: None,
            id: AgentRegistryIdentifier {
                name: "discord".to_string(),
                version: "0.1.0".to_string(),
            },
            name: "discord".to_string(),
            options,
            provider: GraphAgentProvider::Local {
                runtime: RuntimeId::Executable,
            },
            system_prompt: None,
        }
    }

    fn coral_context_graph_request(&self) -> GraphAgentRequest {
        GraphAgentRequest {
            blocking: Some(true),
            coral_plugins: vec![],
            custom_tool_access: vec![],
            description: Some("An agent with access to all the Coral documentation".to_string()),
            id: AgentRegistryIdentifier {
                name: "ca-context7".to_string(),
                version: "0.1.0".to_string(),
            },
            name: "ctx-coral".to_string(),
            options: HashMap::from([
                ("ENABLE_TELEMETRY".to_string(), AgentOptionValue::String("true".to_string())),
                ("LIBRARY_ID".to_string(),  AgentOptionValue::String("websites/coralprotocol".to_string())),
                ("OPENROUTER_API_KEY".to_string(), AgentOptionValue::String(self.arguments.openrouter_api_key.clone())),
            ]),
            provider: GraphAgentProvider::Local {
                runtime: RuntimeId::Docker,
            },
            system_prompt: None,
        }
    }

    pub async fn execute(&self) -> Result<ResponseValue<SessionIdentifier>, Error<RouteException>> {
        let coral_ctx = self.coral_context_graph_request();
        let discord = self.discord_graph_request();

        Client::new(self.arguments.coral_server.as_str())
            .create_session(&SessionRequest {
                agent_graph_request: AgentGraphRequest {
                    groups: vec![vec![coral_ctx.name.clone(), discord.name.clone()]],
                    agents: vec![coral_ctx, discord],
                    custom_tools: Default::default(),
                },
                application_id: "coral-example-app".to_string(),
                privacy_key: "unused".to_string(),
                session_id: None,
            })
            .await
    }
}
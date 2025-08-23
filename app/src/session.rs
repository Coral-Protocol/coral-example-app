use std::collections::HashMap;
use coral_rs::api::generated::{Client, Error, ResponseValue};
use coral_rs::api::generated::types::{AgentGraphRequest, AgentOptionValue, CreateSessionRequest, CreateSessionResponse, GraphAgentProvider, GraphAgentRequest, RouteException, RuntimeId};
use crate::Arguments;

struct AgentDefinition {
    name: String,
    options: HashMap<String, AgentOptionValue>,
}

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

    fn agents(&self) -> Vec<AgentDefinition> {
        vec![
            // AgentDefinition {
            //     name: "context".to_string(),
            //     options: HashMap::new(),
            // },
            AgentDefinition {
                name: "discord".to_string(),
                options: HashMap::from([
                    ("DISCORD_API_TOKEN".to_string(), AgentOptionValue::String(self.arguments.discord_api_token.clone())),
                    ("OPENAI_API_KEY".to_string(), AgentOptionValue::String(self.arguments.openai_api_key.clone())),
                    ("DISCORD_THREAD_ID".to_string(), AgentOptionValue::String(self.channel_id.clone()))
                ]),
            },
        ]
    }

    pub async fn execute(&self) -> Result<ResponseValue<CreateSessionResponse>, Error<RouteException>> {
        let agents = self.agents();
        Client::new(self.arguments.coral_server.as_str())
            .create_session(&CreateSessionRequest {
                agent_graph: Some(AgentGraphRequest {
                    agents: agents
                        .iter()
                        .map(|agent| {
                            (agent.name.clone(), GraphAgentRequest {
                                agent_name: agent.name.clone(),
                                blocking: Some(true),
                                options: agent.options.clone(),
                                provider: GraphAgentProvider::Local {
                                    runtime: RuntimeId::Executable,
                                },
                                system_prompt: None,
                                tools: vec![],
                            })
                        })
                        .collect::<HashMap<String, GraphAgentRequest>>(),

                    // All agents should have access to each other at the moment
                    links: vec![
                        agents
                            .iter()
                            .map(|agent| agent.name.clone())
                            .collect()
                    ],
                    tools: Default::default(),
                }),
                application_id: "coral-example-app".to_string(),
                privacy_key: "private".to_string(),
                session_id: None
            })
            .await
    }
}
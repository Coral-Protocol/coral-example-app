use std::collections::HashMap;
use coral_rs::api::{Client, Error, ResponseValue};
use coral_rs::api::types::{AgentGraphRequest, AgentOptionValue, CreateSessionRequest, CreateSessionResponse, GraphAgentProvider, GraphAgentRequest, RuntimeId};
use crate::Arguments;

struct AgentDefinition {
    name: String,
    options: HashMap<String, AgentOptionValue>,
}

pub struct Session<'a> {
    arguments: &'a Arguments,
    thread_id: String,
    channel_id: String,
}

impl<'a> Session<'a> {
    pub fn new(
        arguments: &'a Arguments,
        thread_id: String,
        channel_id: String,
    ) -> Self {
        Self {
            arguments,
            thread_id,
            channel_id,
        }
    }

    fn agents(&self) -> Vec<AgentDefinition> {
        vec![
            AgentDefinition {
                name: "context".to_string(),
                options: HashMap::new(),
            },
            AgentDefinition {
                name: "discord".to_string(),
                options: HashMap::from([(
                        "DISCORD_API_TOKEN".to_string(),
                        AgentOptionValue::String(self.arguments.discord_api_token.clone())
                    ), (
                        "DISCORD_CHANNEL_ID".to_string(),
                        AgentOptionValue::String(self.channel_id.clone())
                    ), (
                        "DISCORD_THREAD_ID".to_string(),
                        AgentOptionValue::String(self.thread_id.clone())
                    ),
                ]),
            },
        ]
    }

    pub async fn execute(&self) -> Result<ResponseValue<CreateSessionResponse>, Error> {
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
                                options: Default::default(),
                                provider: GraphAgentProvider::Local {
                                    runtime: RuntimeId::Executable,
                                },
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
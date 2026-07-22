use super::*;

#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) struct SetBreakpoints {
    pub(crate) source: dap::Source,
    pub(crate) breakpoints: Vec<SourceBreakpoint>,
    pub(crate) source_modified: Option<bool>,
}

impl LocalDapCommand for SetBreakpoints {
    type Response = Vec<dap::Breakpoint>;
    type DapRequest = dap::requests::SetBreakpoints;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::SetBreakpointsArguments {
            lines: None,
            source_modified: self.source_modified,
            source: self.source.clone(),
            breakpoints: Some(self.breakpoints.clone()),
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.breakpoints)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum DataBreakpointContext {
    Variable {
        variables_reference: u64,
        name: String,
        bytes: Option<u64>,
    },
    Expression {
        expression: String,
        frame_id: Option<u64>,
    },
    Address {
        address: String,
        bytes: Option<u64>,
    },
}

impl DataBreakpointContext {
    pub fn human_readable_label(&self) -> String {
        match self {
            DataBreakpointContext::Variable { name, .. } => format!("Variable: {}", name),
            DataBreakpointContext::Expression { expression, .. } => {
                format!("Expression: {}", expression)
            }
            DataBreakpointContext::Address { address, bytes } => {
                let mut label = format!("Address: {}", address);
                if let Some(bytes) = bytes {
                    label.push_str(&format!(
                        " ({} byte{})",
                        bytes,
                        if *bytes == 1 { "" } else { "s" }
                    ));
                }
                label
            }
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct DataBreakpointInfoCommand {
    pub context: Arc<DataBreakpointContext>,
    pub mode: Option<String>,
}

impl LocalDapCommand for DataBreakpointInfoCommand {
    type Response = dap::DataBreakpointInfoResponse;
    type DapRequest = dap::requests::DataBreakpointInfo;
    const CACHEABLE: bool = true;

    // todo(debugger): We should expand this trait in the future to take a &self
    // Depending on this command is_supported could be differentb
    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_data_breakpoints.unwrap_or(false)
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        let (variables_reference, name, frame_id, as_address, bytes) = match &*self.context {
            DataBreakpointContext::Variable {
                variables_reference,
                name,
                bytes,
            } => (
                Some(*variables_reference),
                name.clone(),
                None,
                Some(false),
                *bytes,
            ),
            DataBreakpointContext::Expression {
                expression,
                frame_id,
            } => (None, expression.clone(), *frame_id, Some(false), None),
            DataBreakpointContext::Address { address, bytes } => {
                (None, address.clone(), None, Some(true), *bytes)
            }
        };

        dap::DataBreakpointInfoArguments {
            variables_reference,
            name,
            frame_id,
            bytes,
            as_address,
            mode: self.mode.clone(),
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct SetDataBreakpointsCommand {
    pub breakpoints: Vec<dap::DataBreakpoint>,
}

impl LocalDapCommand for SetDataBreakpointsCommand {
    type Response = Vec<dap::Breakpoint>;
    type DapRequest = dap::requests::SetDataBreakpoints;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_data_breakpoints.unwrap_or(false)
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::SetDataBreakpointsArguments {
            breakpoints: self.breakpoints.clone(),
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.breakpoints)
    }
}

#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) enum SetExceptionBreakpoints {
    Plain {
        filters: Vec<String>,
    },
    WithOptions {
        filters: Vec<ExceptionFilterOptions>,
    },
}

impl LocalDapCommand for SetExceptionBreakpoints {
    type Response = Vec<dap::Breakpoint>;
    type DapRequest = dap::requests::SetExceptionBreakpoints;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        match self {
            SetExceptionBreakpoints::Plain { filters } => dap::SetExceptionBreakpointsArguments {
                filters: filters.clone(),
                exception_options: None,
                filter_options: None,
            },
            SetExceptionBreakpoints::WithOptions { filters } => {
                dap::SetExceptionBreakpointsArguments {
                    filters: vec![],
                    filter_options: Some(filters.clone()),
                    exception_options: None,
                }
            }
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.breakpoints.unwrap_or_default())
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct LocationsCommand {
    pub(crate) reference: u64,
}

impl LocalDapCommand for LocationsCommand {
    type Response = dap::LocationsResponse;
    type DapRequest = dap::requests::Locations;
    const CACHEABLE: bool = true;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::LocationsArguments {
            location_reference: self.reference,
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

impl DapCommand for LocationsCommand {
    type ProtoRequest = proto::DapLocationsRequest;
    type ProtoResponse = proto::DapLocationsResponse;

    fn client_id_from_proto(message: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(message.session_id)
    }

    fn from_proto(message: &Self::ProtoRequest) -> Self {
        Self {
            reference: message.location_reference,
        }
    }

    fn to_proto(&self, session_id: SessionId, project_id: u64) -> Self::ProtoRequest {
        proto::DapLocationsRequest {
            project_id,
            session_id: session_id.to_proto(),
            location_reference: self.reference,
        }
    }

    fn response_to_proto(_: SessionId, response: Self::Response) -> Self::ProtoResponse {
        proto::DapLocationsResponse {
            source: Some(response.source.to_proto()),
            line: response.line,
            column: response.column,
            end_line: response.end_line,
            end_column: response.end_column,
        }
    }

    fn response_from_proto(&self, response: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(dap::LocationsResponse {
            source: response
                .source
                .map(<dap::Source as ProtoConversion>::from_proto)
                .context("Missing `source` field in Locations proto")?,
            line: response.line,
            column: response.column,
            end_line: response.end_line,
            end_column: response.end_column,
        })
    }
}

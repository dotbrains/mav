use super::*;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct VariablesCommand {
    pub variables_reference: u64,
    pub filter: Option<VariablesArgumentsFilter>,
    pub start: Option<u64>,
    pub count: Option<u64>,
    pub format: Option<ValueFormat>,
}

impl LocalDapCommand for VariablesCommand {
    type Response = Vec<Variable>;
    type DapRequest = dap::requests::Variables;
    const CACHEABLE: bool = true;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::VariablesArguments {
            variables_reference: self.variables_reference,
            filter: self.filter,
            start: self.start,
            count: self.count,
            format: self.format.clone(),
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.variables)
    }
}

impl DapCommand for VariablesCommand {
    type ProtoRequest = proto::VariablesRequest;
    type ProtoResponse = proto::DapVariables;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        proto::VariablesRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            variables_reference: self.variables_reference,
            filter: None,
            start: self.start,
            count: self.count,
            format: None,
        }
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            variables_reference: request.variables_reference,
            filter: None,
            start: request.start,
            count: request.count,
            format: None,
        }
    }

    fn response_to_proto(
        debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapVariables {
            client_id: debug_client_id.to_proto(),
            variables: message.to_proto(),
        }
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(Vec::from_proto(message.variables))
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct SetVariableValueCommand {
    pub name: String,
    pub value: String,
    pub variables_reference: u64,
}
impl LocalDapCommand for SetVariableValueCommand {
    type Response = SetVariableResponse;
    type DapRequest = dap::requests::SetVariable;
    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_set_variable.unwrap_or_default()
    }
    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::SetVariableArguments {
            format: None,
            name: self.name.clone(),
            value: self.value.clone(),
            variables_reference: self.variables_reference,
        }
    }
    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

impl DapCommand for SetVariableValueCommand {
    type ProtoRequest = proto::DapSetVariableValueRequest;
    type ProtoResponse = proto::DapSetVariableValueResponse;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        proto::DapSetVariableValueRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            variables_reference: self.variables_reference,
            value: self.value.clone(),
            name: self.name.clone(),
        }
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            variables_reference: request.variables_reference,
            name: request.name.clone(),
            value: request.value.clone(),
        }
    }

    fn response_to_proto(
        debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapSetVariableValueResponse {
            client_id: debug_client_id.to_proto(),
            value: message.value,
            variable_type: message.type_,
            named_variables: message.named_variables,
            variables_reference: message.variables_reference,
            indexed_variables: message.indexed_variables,
            memory_reference: message.memory_reference,
        }
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(SetVariableResponse {
            value: message.value,
            type_: message.variable_type,
            variables_reference: message.variables_reference,
            named_variables: message.named_variables,
            indexed_variables: message.indexed_variables,
            memory_reference: message.memory_reference,
            value_location_reference: None, // TODO
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct RestartStackFrameCommand {
    pub stack_frame_id: u64,
}

impl LocalDapCommand for RestartStackFrameCommand {
    type Response = <dap::requests::RestartFrame as dap::requests::Request>::Response;
    type DapRequest = dap::requests::RestartFrame;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_restart_frame.unwrap_or_default()
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::RestartFrameArguments {
            frame_id: self.stack_frame_id,
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for RestartStackFrameCommand {
    type ProtoRequest = proto::DapRestartStackFrameRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            stack_frame_id: request.stack_frame_id,
        }
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapRestartStackFrameRequest {
        proto::DapRestartStackFrameRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            stack_frame_id: self.stack_frame_id,
        }
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        _message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::Ack {}
    }

    fn response_from_proto(&self, _message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(())
    }
}

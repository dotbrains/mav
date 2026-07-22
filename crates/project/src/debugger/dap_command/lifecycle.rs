use super::*;

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct DisconnectCommand {
    pub restart: Option<bool>,
    pub terminate_debuggee: Option<bool>,
    pub suspend_debuggee: Option<bool>,
}

impl LocalDapCommand for DisconnectCommand {
    type Response = <dap::requests::Disconnect as dap::requests::Request>::Response;
    type DapRequest = dap::requests::Disconnect;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::DisconnectArguments {
            restart: self.restart,
            terminate_debuggee: self.terminate_debuggee,
            suspend_debuggee: self.suspend_debuggee,
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for DisconnectCommand {
    type ProtoRequest = proto::DapDisconnectRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            restart: request.restart,
            terminate_debuggee: request.terminate_debuggee,
            suspend_debuggee: request.suspend_debuggee,
        }
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapDisconnectRequest {
        proto::DapDisconnectRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            restart: self.restart,
            terminate_debuggee: self.terminate_debuggee,
            suspend_debuggee: self.suspend_debuggee,
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

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct TerminateThreadsCommand {
    pub thread_ids: Option<Vec<i64>>,
}

impl LocalDapCommand for TerminateThreadsCommand {
    type Response = <dap::requests::TerminateThreads as dap::requests::Request>::Response;
    type DapRequest = dap::requests::TerminateThreads;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities
            .supports_terminate_threads_request
            .unwrap_or_default()
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::TerminateThreadsArguments {
            thread_ids: self.thread_ids.clone(),
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for TerminateThreadsCommand {
    type ProtoRequest = proto::DapTerminateThreadsRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        let thread_ids = if request.thread_ids.is_empty() {
            None
        } else {
            Some(request.thread_ids.clone())
        };

        Self { thread_ids }
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapTerminateThreadsRequest {
        proto::DapTerminateThreadsRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_ids: self.thread_ids.clone().unwrap_or_default(),
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

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct TerminateCommand {
    pub restart: Option<bool>,
}

impl LocalDapCommand for TerminateCommand {
    type Response = <dap::requests::Terminate as dap::requests::Request>::Response;
    type DapRequest = dap::requests::Terminate;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_terminate_request.unwrap_or_default()
    }
    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::TerminateArguments {
            restart: self.restart,
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for TerminateCommand {
    type ProtoRequest = proto::DapTerminateRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            restart: request.restart,
        }
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapTerminateRequest {
        proto::DapTerminateRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            restart: self.restart,
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

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct RestartCommand {
    pub raw: serde_json::Value,
}

impl LocalDapCommand for RestartCommand {
    type Response = <dap::requests::Restart as dap::requests::Request>::Response;
    type DapRequest = dap::requests::Restart;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_restart_request.unwrap_or_default()
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::RestartArguments {
            raw: self.raw.clone(),
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for RestartCommand {
    type ProtoRequest = proto::DapRestartRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            raw: serde_json::from_slice(&request.raw_args)
                .log_err()
                .unwrap_or(serde_json::Value::Null),
        }
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapRestartRequest {
        let raw_args = serde_json::to_vec(&self.raw).log_err().unwrap_or_default();

        proto::DapRestartRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            raw_args,
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

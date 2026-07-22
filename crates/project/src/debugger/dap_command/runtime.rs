use super::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct ThreadsCommand;

impl LocalDapCommand for ThreadsCommand {
    type Response = Vec<dap::Thread>;
    type DapRequest = dap::requests::Threads;
    const CACHEABLE: bool = true;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::ThreadsArgument {}
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.threads)
    }
}

impl DapCommand for ThreadsCommand {
    type ProtoRequest = proto::DapThreadsRequest;
    type ProtoResponse = proto::DapThreadsResponse;

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        proto::DapThreadsRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
        }
    }

    fn from_proto(_request: &Self::ProtoRequest) -> Self {
        Self {}
    }

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(Vec::from_proto(message.threads))
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapThreadsResponse {
            threads: message.to_proto(),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) struct Initialize {
    pub(crate) adapter_id: String,
}

fn dap_client_capabilities(adapter_id: String) -> InitializeRequestArguments {
    InitializeRequestArguments {
        client_id: Some("mav".to_owned()),
        client_name: Some("Mav".to_owned()),
        adapter_id,
        locale: Some("en-US".to_owned()),
        path_format: Some(InitializeRequestArgumentsPathFormat::Path),
        supports_variable_type: Some(true),
        supports_variable_paging: Some(false),
        supports_run_in_terminal_request: Some(true),
        supports_memory_references: Some(true),
        supports_progress_reporting: Some(false),
        supports_invalidated_event: Some(false),
        lines_start_at1: Some(true),
        columns_start_at1: Some(true),
        supports_memory_event: Some(false),
        supports_args_can_be_interpreted_by_shell: Some(false),
        supports_start_debugging_request: Some(true),
        supports_ansistyling: Some(true),
    }
}

impl LocalDapCommand for Initialize {
    type Response = Capabilities;
    type DapRequest = dap::requests::Initialize;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap_client_capabilities(self.adapter_id.clone())
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) struct ConfigurationDone {}

impl LocalDapCommand for ConfigurationDone {
    type Response = ();
    type DapRequest = dap::requests::ConfigurationDone;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities
            .supports_configuration_done_request
            .unwrap_or_default()
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::ConfigurationDoneArguments {}
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) struct Launch {
    pub(crate) raw: Value,
}

impl LocalDapCommand for Launch {
    type Response = ();
    type DapRequest = dap::requests::Launch;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::LaunchRequestArguments {
            raw: self.raw.clone(),
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) struct Attach {
    pub(crate) raw: Value,
}

impl LocalDapCommand for Attach {
    type Response = ();
    type DapRequest = dap::requests::Attach;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::AttachRequestArguments {
            raw: self.raw.clone(),
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

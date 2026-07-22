use super::*;

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct StepCommand {
    pub thread_id: i64,
    pub granularity: Option<SteppingGranularity>,
    pub single_thread: Option<bool>,
}

impl StepCommand {
    fn from_proto(message: proto::DapNextRequest) -> Self {
        const LINE: i32 = proto::SteppingGranularity::Line as i32;
        const INSTRUCTION: i32 = proto::SteppingGranularity::Instruction as i32;

        let granularity = message.granularity.map(|granularity| match granularity {
            LINE => SteppingGranularity::Line,
            INSTRUCTION => SteppingGranularity::Instruction,
            _ => SteppingGranularity::Statement,
        });

        Self {
            thread_id: message.thread_id,
            granularity,
            single_thread: message.single_thread,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct NextCommand {
    pub inner: StepCommand,
}

impl LocalDapCommand for NextCommand {
    type Response = <Next as dap::requests::Request>::Response;
    type DapRequest = Next;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        NextArguments {
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity,
        }
    }
    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for NextCommand {
    type ProtoRequest = proto::DapNextRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            inner: StepCommand::from_proto(request.clone()),
        }
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        _message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::Ack {}
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapNextRequest {
        proto::DapNextRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity.map(|gran| gran.to_proto() as i32),
        }
    }

    fn response_from_proto(&self, _message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(())
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct StepInCommand {
    pub inner: StepCommand,
}

impl LocalDapCommand for StepInCommand {
    type Response = <dap::requests::StepIn as dap::requests::Request>::Response;
    type DapRequest = dap::requests::StepIn;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        StepInArguments {
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            target_id: None,
            granularity: self.inner.granularity,
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for StepInCommand {
    type ProtoRequest = proto::DapStepInRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            inner: StepCommand::from_proto(proto::DapNextRequest {
                project_id: request.project_id,
                client_id: request.client_id,
                thread_id: request.thread_id,
                single_thread: request.single_thread,
                granularity: request.granularity,
            }),
        }
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        _message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::Ack {}
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapStepInRequest {
        proto::DapStepInRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity.map(|gran| gran.to_proto() as i32),
            target_id: None,
        }
    }

    fn response_from_proto(&self, _message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(())
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct StepOutCommand {
    pub inner: StepCommand,
}

impl LocalDapCommand for StepOutCommand {
    type Response = <dap::requests::StepOut as dap::requests::Request>::Response;
    type DapRequest = dap::requests::StepOut;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        StepOutArguments {
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity,
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for StepOutCommand {
    type ProtoRequest = proto::DapStepOutRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            inner: StepCommand::from_proto(proto::DapNextRequest {
                project_id: request.project_id,
                client_id: request.client_id,
                thread_id: request.thread_id,
                single_thread: request.single_thread,
                granularity: request.granularity,
            }),
        }
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        _message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::Ack {}
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapStepOutRequest {
        proto::DapStepOutRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity.map(|gran| gran.to_proto() as i32),
        }
    }

    fn response_from_proto(&self, _message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(())
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct StepBackCommand {
    pub inner: StepCommand,
}
impl LocalDapCommand for StepBackCommand {
    type Response = <dap::requests::StepBack as dap::requests::Request>::Response;
    type DapRequest = dap::requests::StepBack;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_step_back.unwrap_or_default()
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::StepBackArguments {
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity,
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for StepBackCommand {
    type ProtoRequest = proto::DapStepBackRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            inner: StepCommand::from_proto(proto::DapNextRequest {
                project_id: request.project_id,
                client_id: request.client_id,
                thread_id: request.thread_id,
                single_thread: request.single_thread,
                granularity: request.granularity,
            }),
        }
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        _message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::Ack {}
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapStepBackRequest {
        proto::DapStepBackRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.inner.thread_id,
            single_thread: self.inner.single_thread,
            granularity: self.inner.granularity.map(|gran| gran.to_proto() as i32),
        }
    }

    fn response_from_proto(&self, _message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(())
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct ContinueCommand {
    pub args: ContinueArguments,
}

impl LocalDapCommand for ContinueCommand {
    type Response = <Continue as dap::requests::Request>::Response;
    type DapRequest = Continue;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        self.args.clone()
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

impl DapCommand for ContinueCommand {
    type ProtoRequest = proto::DapContinueRequest;
    type ProtoResponse = proto::DapContinueResponse;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapContinueRequest {
        proto::DapContinueRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.args.thread_id,
            single_thread: self.args.single_thread,
        }
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            args: ContinueArguments {
                thread_id: request.thread_id,
                single_thread: request.single_thread,
            },
        }
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(Self::Response {
            all_threads_continued: message.all_threads_continued,
        })
    }

    fn response_to_proto(
        debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapContinueResponse {
            client_id: debug_client_id.to_proto(),
            all_threads_continued: message.all_threads_continued,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) struct PauseCommand {
    pub thread_id: i64,
}

impl LocalDapCommand for PauseCommand {
    type Response = <dap::requests::Pause as dap::requests::Request>::Response;
    type DapRequest = dap::requests::Pause;
    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::PauseArguments {
            thread_id: self.thread_id,
        }
    }

    fn response_from_dap(
        &self,
        _message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(())
    }
}

impl DapCommand for PauseCommand {
    type ProtoRequest = proto::DapPauseRequest;
    type ProtoResponse = proto::Ack;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            thread_id: request.thread_id,
        }
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapPauseRequest {
        proto::DapPauseRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.thread_id,
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

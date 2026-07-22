use super::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct ModulesCommand;

impl LocalDapCommand for ModulesCommand {
    type Response = Vec<dap::Module>;
    type DapRequest = dap::requests::Modules;
    const CACHEABLE: bool = true;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities.supports_modules_request.unwrap_or_default()
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::ModulesArguments {
            start_module: None,
            module_count: None,
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.modules)
    }
}

impl DapCommand for ModulesCommand {
    type ProtoRequest = proto::DapModulesRequest;
    type ProtoResponse = proto::DapModulesResponse;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(_request: &Self::ProtoRequest) -> Self {
        Self {}
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapModulesRequest {
        proto::DapModulesRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
        }
    }

    fn response_to_proto(
        debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapModulesResponse {
            modules: message
                .into_iter()
                .map(|module| module.to_proto())
                .collect(),
            client_id: debug_client_id.to_proto(),
        }
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(message
            .modules
            .into_iter()
            .filter_map(|module| dap::Module::from_proto(module).ok())
            .collect())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct LoadedSourcesCommand;

impl LocalDapCommand for LoadedSourcesCommand {
    type Response = Vec<dap::Source>;
    type DapRequest = dap::requests::LoadedSources;
    const CACHEABLE: bool = true;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities
            .supports_loaded_sources_request
            .unwrap_or_default()
    }
    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::LoadedSourcesArguments {}
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.sources)
    }
}

impl DapCommand for LoadedSourcesCommand {
    type ProtoRequest = proto::DapLoadedSourcesRequest;
    type ProtoResponse = proto::DapLoadedSourcesResponse;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(_request: &Self::ProtoRequest) -> Self {
        Self {}
    }

    fn to_proto(
        &self,
        debug_client_id: SessionId,
        upstream_project_id: u64,
    ) -> proto::DapLoadedSourcesRequest {
        proto::DapLoadedSourcesRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
        }
    }

    fn response_to_proto(
        debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapLoadedSourcesResponse {
            sources: message
                .into_iter()
                .map(|source| source.to_proto())
                .collect(),
            client_id: debug_client_id.to_proto(),
        }
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(message
            .sources
            .into_iter()
            .map(dap::Source::from_proto)
            .collect())
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct StackTraceCommand {
    pub thread_id: i64,
    pub start_frame: Option<u64>,
    pub levels: Option<u64>,
}

impl LocalDapCommand for StackTraceCommand {
    type Response = Vec<dap::StackFrame>;
    type DapRequest = dap::requests::StackTrace;
    const CACHEABLE: bool = true;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::StackTraceArguments {
            thread_id: self.thread_id,
            start_frame: self.start_frame,
            levels: self.levels,
            format: None,
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.stack_frames)
    }
}

impl DapCommand for StackTraceCommand {
    type ProtoRequest = proto::DapStackTraceRequest;
    type ProtoResponse = proto::DapStackTraceResponse;

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        proto::DapStackTraceRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            thread_id: self.thread_id,
            start_frame: self.start_frame,
            stack_trace_levels: self.levels,
        }
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            thread_id: request.thread_id,
            start_frame: request.start_frame,
            levels: request.stack_trace_levels,
        }
    }

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(message
            .frames
            .into_iter()
            .map(dap::StackFrame::from_proto)
            .collect())
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapStackTraceResponse {
            frames: message.to_proto(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct ScopesCommand {
    pub stack_frame_id: u64,
}

impl LocalDapCommand for ScopesCommand {
    type Response = Vec<dap::Scope>;
    type DapRequest = dap::requests::Scopes;
    const CACHEABLE: bool = true;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::ScopesArguments {
            frame_id: self.stack_frame_id,
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message.scopes)
    }
}

impl DapCommand for ScopesCommand {
    type ProtoRequest = proto::DapScopesRequest;
    type ProtoResponse = proto::DapScopesResponse;

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        proto::DapScopesRequest {
            project_id: upstream_project_id,
            client_id: debug_client_id.to_proto(),
            stack_frame_id: self.stack_frame_id,
        }
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            stack_frame_id: request.stack_frame_id,
        }
    }

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(Vec::from_proto(message.scopes))
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapScopesResponse {
            scopes: message.to_proto(),
        }
    }
}

impl LocalDapCommand for crate::debugger::session::CompletionsQuery {
    type Response = dap::CompletionsResponse;
    type DapRequest = dap::requests::Completions;
    const CACHEABLE: bool = true;

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::CompletionsArguments {
            text: self.query.clone(),
            frame_id: self.frame_id,
            column: self.column,
            line: None,
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities
            .supports_completions_request
            .unwrap_or_default()
    }
}

impl DapCommand for crate::debugger::session::CompletionsQuery {
    type ProtoRequest = proto::DapCompletionRequest;
    type ProtoResponse = proto::DapCompletionResponse;

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        proto::DapCompletionRequest {
            client_id: debug_client_id.to_proto(),
            project_id: upstream_project_id,
            frame_id: self.frame_id,
            query: self.query.clone(),
            column: self.column,
            line: self.line,
        }
    }

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            query: request.query.clone(),
            frame_id: request.frame_id,
            column: request.column,
            line: request.line,
        }
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(dap::CompletionsResponse {
            targets: Vec::from_proto(message.completions),
        })
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapCompletionResponse {
            client_id: _debug_client_id.to_proto(),
            completions: message.targets.to_proto(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct EvaluateCommand {
    pub expression: String,
    pub frame_id: Option<u64>,
    pub context: Option<dap::EvaluateArgumentsContext>,
    pub source: Option<dap::Source>,
}

impl LocalDapCommand for EvaluateCommand {
    type Response = dap::EvaluateResponse;
    type DapRequest = dap::requests::Evaluate;
    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::EvaluateArguments {
            expression: self.expression.clone(),
            frame_id: self.frame_id,
            context: self.context.clone(),
            source: self.source.clone(),
            line: None,
            column: None,
            format: None,
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}
impl DapCommand for EvaluateCommand {
    type ProtoRequest = proto::DapEvaluateRequest;
    type ProtoResponse = proto::DapEvaluateResponse;

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        proto::DapEvaluateRequest {
            client_id: debug_client_id.to_proto(),
            project_id: upstream_project_id,
            expression: self.expression.clone(),
            frame_id: self.frame_id,
            context: self
                .context
                .clone()
                .map(|context| context.to_proto().into()),
        }
    }

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        SessionId::from_proto(request.client_id)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Self {
            expression: request.expression.clone(),
            frame_id: request.frame_id,
            context: Some(dap::EvaluateArgumentsContext::from_proto(request.context())),
            source: None,
        }
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        Ok(dap::EvaluateResponse {
            result: message.result.clone(),
            type_: message.evaluate_type.clone(),
            presentation_hint: None,
            variables_reference: message.variable_reference,
            named_variables: message.named_variables,
            indexed_variables: message.indexed_variables,
            memory_reference: message.memory_reference,
            value_location_reference: None, //TODO
        })
    }

    fn response_to_proto(
        _debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        proto::DapEvaluateResponse {
            result: message.result,
            evaluate_type: message.type_,
            variable_reference: message.variables_reference,
            named_variables: message.named_variables,
            indexed_variables: message.indexed_variables,
            memory_reference: message.memory_reference,
        }
    }
}

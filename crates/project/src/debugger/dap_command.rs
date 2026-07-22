use std::sync::Arc;

use anyhow::{Context as _, Ok, Result};
use base64::Engine;
use dap::{
    Capabilities, ContinueArguments, ExceptionFilterOptions, InitializeRequestArguments,
    InitializeRequestArgumentsPathFormat, NextArguments, SetVariableResponse, SourceBreakpoint,
    StepInArguments, StepOutArguments, SteppingGranularity, ValueFormat, Variable,
    VariablesArgumentsFilter,
    client::SessionId,
    proto_conversions::ProtoConversion,
    requests::{Continue, Next},
};

use rpc::proto;
use serde_json::Value;
use util::ResultExt;

pub trait LocalDapCommand: 'static + Send + Sync + std::fmt::Debug {
    type Response: 'static + Send + std::fmt::Debug;
    type DapRequest: 'static + Send + dap::requests::Request;
    /// Is this request idempotent? Is it safe to cache the response for as long as the execution environment is unchanged?
    const CACHEABLE: bool = false;

    fn is_supported(_capabilities: &Capabilities) -> bool {
        true
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments;

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response>;
}

pub trait DapCommand: LocalDapCommand {
    type ProtoRequest: 'static + Send;
    type ProtoResponse: 'static + Send;

    #[allow(dead_code)]
    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId;

    #[allow(dead_code)]
    fn from_proto(request: &Self::ProtoRequest) -> Self;

    #[allow(unused)]
    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest;

    #[allow(dead_code)]
    fn response_to_proto(
        debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse;

    #[allow(unused)]
    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response>;
}

impl<T: LocalDapCommand> LocalDapCommand for Arc<T> {
    type Response = T::Response;
    type DapRequest = T::DapRequest;

    fn is_supported(capabilities: &Capabilities) -> bool {
        T::is_supported(capabilities)
    }

    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        T::to_dap(self)
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        T::response_from_dap(self, message)
    }
}

impl<T: DapCommand> DapCommand for Arc<T> {
    type ProtoRequest = T::ProtoRequest;
    type ProtoResponse = T::ProtoResponse;

    fn client_id_from_proto(request: &Self::ProtoRequest) -> SessionId {
        T::client_id_from_proto(request)
    }

    fn from_proto(request: &Self::ProtoRequest) -> Self {
        Arc::new(T::from_proto(request))
    }

    fn to_proto(&self, debug_client_id: SessionId, upstream_project_id: u64) -> Self::ProtoRequest {
        T::to_proto(self, debug_client_id, upstream_project_id)
    }

    fn response_to_proto(
        debug_client_id: SessionId,
        message: Self::Response,
    ) -> Self::ProtoResponse {
        T::response_to_proto(debug_client_id, message)
    }

    fn response_from_proto(&self, message: Self::ProtoResponse) -> Result<Self::Response> {
        T::response_from_proto(self, message)
    }
}

mod breakpoints;
mod lifecycle;
mod memory;
mod runtime;
mod stack_source;
mod stepping;
mod variables;

pub(crate) use breakpoints::{
    DataBreakpointContext, DataBreakpointInfoCommand, SetDataBreakpointsCommand,
};
pub(crate) use breakpoints::{LocationsCommand, SetBreakpoints, SetExceptionBreakpoints};
pub(crate) use lifecycle::{
    DisconnectCommand, RestartCommand, TerminateCommand, TerminateThreadsCommand,
};
pub(crate) use memory::{ReadMemory, ReadMemoryResponse};
pub(crate) use runtime::ThreadsCommand;
pub(crate) use runtime::{Attach, ConfigurationDone, Initialize, Launch};
pub(crate) use stack_source::{
    EvaluateCommand, LoadedSourcesCommand, ModulesCommand, ScopesCommand, StackTraceCommand,
};
pub(crate) use stepping::{
    ContinueCommand, NextCommand, PauseCommand, StepBackCommand, StepCommand, StepInCommand,
    StepOutCommand,
};
pub(crate) use variables::{RestartStackFrameCommand, SetVariableValueCommand, VariablesCommand};

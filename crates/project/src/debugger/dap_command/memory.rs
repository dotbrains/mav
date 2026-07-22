use super::*;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ReadMemory {
    pub(crate) memory_reference: String,
    pub(crate) offset: Option<u64>,
    pub(crate) count: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReadMemoryResponse {
    pub(crate) address: Arc<str>,
    pub(crate) unreadable_bytes: Option<u64>,
    pub(crate) content: Arc<[u8]>,
}

impl LocalDapCommand for ReadMemory {
    type Response = ReadMemoryResponse;
    type DapRequest = dap::requests::ReadMemory;
    const CACHEABLE: bool = true;

    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities
            .supports_read_memory_request
            .unwrap_or_default()
    }
    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        dap::ReadMemoryArguments {
            memory_reference: self.memory_reference.clone(),
            offset: self.offset,
            count: self.count,
        }
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        let data = if let Some(data) = message.data {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .log_err()
                .context("parsing base64 data from DAP's ReadMemory response")?
        } else {
            vec![]
        };

        Ok(ReadMemoryResponse {
            address: message.address.into(),
            content: data.into(),
            unreadable_bytes: message.unreadable_bytes,
        })
    }
}

impl LocalDapCommand for dap::WriteMemoryArguments {
    type Response = dap::WriteMemoryResponse;
    type DapRequest = dap::requests::WriteMemory;
    fn is_supported(capabilities: &Capabilities) -> bool {
        capabilities
            .supports_write_memory_request
            .unwrap_or_default()
    }
    fn to_dap(&self) -> <Self::DapRequest as dap::requests::Request>::Arguments {
        self.clone()
    }

    fn response_from_dap(
        &self,
        message: <Self::DapRequest as dap::requests::Request>::Response,
    ) -> Result<Self::Response> {
        Ok(message)
    }
}

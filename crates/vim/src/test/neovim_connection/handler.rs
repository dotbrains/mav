use nvim_rs::{Handler, Neovim, Value};
use tokio::process::ChildStdin;

#[derive(Clone)]
pub(super) struct NvimHandler {}

#[async_trait::async_trait]
impl Handler for NvimHandler {
    type Writer = nvim_rs::compat::tokio::Compat<ChildStdin>;

    async fn handle_request(
        &self,
        _event_name: String,
        _arguments: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) -> Result<Value, Value> {
        unimplemented!();
    }

    async fn handle_notify(
        &self,
        _event_name: String,
        _arguments: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) {
    }
}

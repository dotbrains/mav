use crate::{state::Mode, test::neovim_connection::NeovimConnection};

use super::data::NeovimData;

impl NeovimConnection {
    pub async fn send_keystroke(&mut self, keystroke_text: &str) {
        if matches!(self.data.front(), Some(NeovimData::Get { .. })) {
            self.data.pop_front();
        }
        assert_eq!(
            self.data.pop_front(),
            Some(NeovimData::Key(keystroke_text.to_string())),
            "operation does not match recorded script. re-record with --features=neovim"
        );
    }

    pub async fn set_state(&mut self, marked_text: &str) {
        if let Some(NeovimData::Get { mode, state: text }) = self.data.front() {
            if *mode == Mode::Normal && *text == marked_text {
                return;
            }
            self.data.pop_front();
        }
        assert_eq!(
            self.data.pop_front(),
            Some(NeovimData::Put {
                state: marked_text.to_string()
            }),
            "operation does not match recorded script. re-record with --features=neovim"
        );
    }

    pub async fn set_option(&mut self, value: &str) {
        if let Some(NeovimData::Get { .. }) = self.data.front() {
            self.data.pop_front();
        };
        assert_eq!(
            self.data.pop_front(),
            Some(NeovimData::SetOption {
                value: value.to_string(),
            }),
            "operation does not match recorded script. re-record with --features=neovim"
        );
    }

    pub async fn exec(&mut self, value: &str) {
        if let Some(NeovimData::Get { .. }) = self.data.front() {
            self.data.pop_front();
        };
        assert_eq!(
            self.data.pop_front(),
            Some(NeovimData::Exec {
                command: value.to_string(),
            }),
            "operation does not match recorded script. re-record with --features=neovim"
        );
    }

    pub async fn read_register(&mut self, register: char) -> String {
        if let Some(NeovimData::Get { .. }) = self.data.front() {
            self.data.pop_front();
        };
        if let Some(NeovimData::ReadRegister { name, value }) = self.data.pop_front()
            && name == register
        {
            return value;
        }

        panic!("operation does not match recorded script. re-record with --features=neovim")
    }

    pub async fn state(&mut self) -> (Mode, String) {
        if let Some(NeovimData::Get { state: raw, mode }) = self.data.front() {
            (*mode, raw.to_string())
        } else {
            panic!("operation does not match recorded script. re-record with --features=neovim");
        }
    }
}

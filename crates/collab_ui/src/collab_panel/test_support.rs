use super::*;

impl CollabPanel {
    pub fn entries_as_strings(&self) -> Vec<String> {
        let mut string_entries = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let selected_marker = if self.selection == Some(index) {
                "  <== selected"
            } else {
                ""
            };
            match entry {
                ListEntry::Header(section) => {
                    let name = match section {
                        Section::ActiveCall => "Active Call",
                        Section::FavoriteChannels => "Favorites",
                        Section::Channels => "Channels",
                        Section::ChannelInvites => "Channel Invites",
                        Section::ContactRequests => "Contact Requests",
                        Section::Contacts => "Contacts",
                        Section::Online => "Online",
                        Section::Offline => "Offline",
                    };
                    string_entries.push(format!("[{name}]"));
                }
                ListEntry::Channel {
                    channel,
                    depth,
                    has_children,
                    ..
                } => {
                    let indent = "  ".repeat(*depth + 1);
                    let icon = if *has_children {
                        "v "
                    } else if channel.visibility == proto::ChannelVisibility::Public {
                        "🛜 "
                    } else {
                        "#️⃣ "
                    };
                    string_entries.push(format!("{indent}{icon}{}{selected_marker}", channel.name));
                }
                ListEntry::ChannelNotes { .. } => {
                    string_entries.push(format!("  (notes){selected_marker}"));
                }
                ListEntry::ChannelEditor { depth } => {
                    let indent = "  ".repeat(*depth + 1);
                    string_entries.push(format!("{indent}[editor]{selected_marker}"));
                }
                ListEntry::ChannelInvite(channel) => {
                    string_entries.push(format!("  (invite) #{}{selected_marker}", channel.name));
                }
                ListEntry::CallParticipant { user, .. } => {
                    string_entries.push(format!("  {}{selected_marker}", user.username));
                }
                ListEntry::ParticipantProject {
                    worktree_root_names,
                    ..
                } => {
                    string_entries.push(format!(
                        "    {}{selected_marker}",
                        worktree_root_names.join(", ")
                    ));
                }
                ListEntry::ParticipantScreen { .. } => {
                    string_entries.push(format!("    (screen){selected_marker}"));
                }
                ListEntry::IncomingRequest(user) => {
                    string_entries.push(format!("  (incoming) {}{selected_marker}", user.username));
                }
                ListEntry::OutgoingRequest(user) => {
                    string_entries.push(format!("  (outgoing) {}{selected_marker}", user.username));
                }
                ListEntry::Contact { contact, .. } => {
                    string_entries.push(format!("  {}{selected_marker}", contact.user.username));
                }
                ListEntry::ContactPlaceholder => {}
            }
        }
        string_entries
    }
}

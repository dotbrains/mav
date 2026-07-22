use super::*;

#[derive(Clone, Deserialize, JsonSchema, PartialEq)]
pub(super) enum VimOption {
    Wrap(bool),
    Number(bool),
    RelativeNumber(bool),
    IgnoreCase(bool),
    GDefault(bool),
}

impl VimOption {
    pub(super) fn possible_commands(query: &str) -> Vec<CommandInterceptItem> {
        let mut prefix_of_options = Vec::new();
        let mut options = query.split(" ").collect::<Vec<_>>();
        let prefix = options.pop().unwrap_or_default();
        for option in options {
            if let Some(opt) = Self::from(option) {
                prefix_of_options.push(opt)
            } else {
                return vec![];
            }
        }

        Self::possibilities(prefix)
            .map(|possible| {
                let mut options = prefix_of_options.clone();
                options.push(possible);

                CommandInterceptItem {
                    string: format!(
                        ":set {}",
                        options.iter().map(|opt| opt.to_string()).join(" ")
                    ),
                    action: VimSet { options }.boxed_clone(),
                    positions: vec![],
                }
            })
            .collect()
    }

    fn possibilities(query: &str) -> impl Iterator<Item = Self> + '_ {
        [
            (None, VimOption::Wrap(true)),
            (None, VimOption::Wrap(false)),
            (None, VimOption::Number(true)),
            (None, VimOption::Number(false)),
            (None, VimOption::RelativeNumber(true)),
            (None, VimOption::RelativeNumber(false)),
            (Some("rnu"), VimOption::RelativeNumber(true)),
            (Some("nornu"), VimOption::RelativeNumber(false)),
            (None, VimOption::IgnoreCase(true)),
            (None, VimOption::IgnoreCase(false)),
            (Some("ic"), VimOption::IgnoreCase(true)),
            (Some("noic"), VimOption::IgnoreCase(false)),
            (None, VimOption::GDefault(true)),
            (Some("gd"), VimOption::GDefault(true)),
            (None, VimOption::GDefault(false)),
            (Some("nogd"), VimOption::GDefault(false)),
        ]
        .into_iter()
        .filter(move |(prefix, option)| prefix.unwrap_or(option.to_string()).starts_with(query))
        .map(|(_, option)| option)
    }

    fn from(option: &str) -> Option<Self> {
        match option {
            "wrap" => Some(Self::Wrap(true)),
            "nowrap" => Some(Self::Wrap(false)),

            "number" => Some(Self::Number(true)),
            "nu" => Some(Self::Number(true)),
            "nonumber" => Some(Self::Number(false)),
            "nonu" => Some(Self::Number(false)),

            "relativenumber" => Some(Self::RelativeNumber(true)),
            "rnu" => Some(Self::RelativeNumber(true)),
            "norelativenumber" => Some(Self::RelativeNumber(false)),
            "nornu" => Some(Self::RelativeNumber(false)),

            "ignorecase" => Some(Self::IgnoreCase(true)),
            "ic" => Some(Self::IgnoreCase(true)),
            "noignorecase" => Some(Self::IgnoreCase(false)),
            "noic" => Some(Self::IgnoreCase(false)),

            "gdefault" => Some(Self::GDefault(true)),
            "gd" => Some(Self::GDefault(true)),
            "nogdefault" => Some(Self::GDefault(false)),
            "nogd" => Some(Self::GDefault(false)),

            _ => None,
        }
    }

    fn to_string(&self) -> &'static str {
        match self {
            VimOption::Wrap(true) => "wrap",
            VimOption::Wrap(false) => "nowrap",
            VimOption::Number(true) => "number",
            VimOption::Number(false) => "nonumber",
            VimOption::RelativeNumber(true) => "relativenumber",
            VimOption::RelativeNumber(false) => "norelativenumber",
            VimOption::IgnoreCase(true) => "ignorecase",
            VimOption::IgnoreCase(false) => "noignorecase",
            VimOption::GDefault(true) => "gdefault",
            VimOption::GDefault(false) => "nogdefault",
        }
    }
}

/// Sets vim options and configuration values.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = vim, no_json, no_register)]
pub(super) struct VimSet {
    pub(super) options: Vec<VimOption>,
}

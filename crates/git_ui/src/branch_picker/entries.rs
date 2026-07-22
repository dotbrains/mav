use super::*;

#[derive(Debug, Clone, PartialEq)]
enum Entry {
    Branch {
        branch: Branch,
        positions: Vec<usize>,
    },
    NewUrl {
        url: String,
    },
    NewBranch {
        name: String,
    },
    NewRemoteName {
        name: String,
        url: SharedString,
    },
}

impl Entry {
    fn as_branch(&self) -> Option<&Branch> {
        match self {
            Entry::Branch { branch, .. } => Some(branch),
            _ => None,
        }
    }

    fn name(&self) -> &str {
        match self {
            Entry::Branch { branch, .. } => branch.name(),
            Entry::NewUrl { url, .. } => url.as_str(),
            Entry::NewBranch { name, .. } => name.as_str(),
            Entry::NewRemoteName { name, .. } => name.as_str(),
        }
    }

    #[cfg(test)]
    fn is_new_url(&self) -> bool {
        matches!(self, Self::NewUrl { .. })
    }

    #[cfg(test)]
    fn is_new_branch(&self) -> bool {
        matches!(self, Self::NewBranch { .. })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BranchFilter {
    /// Show both local and remote branches.
    All,
    /// Only show remote branches.
    Remote,
}

impl BranchFilter {
    fn invert(&self) -> Self {
        match self {
            BranchFilter::All => BranchFilter::Remote,
            BranchFilter::Remote => BranchFilter::All,
        }
    }
}

pub struct BranchListDelegate {
    workspace: WeakEntity<Workspace>,
    matches: Vec<Entry>,
    all_branches: Vec<Branch>,
    branch_list_error: Option<SharedString>,
    default_branch: Option<SharedString>,
    repo: Option<Entity<Repository>>,
    style: BranchListStyle,
    selected_index: usize,
    last_query: String,
    modifiers: Modifiers,
    branch_filter: BranchFilter,
    state: PickerState,
    branch_selection_behavior: BranchSelectionBehavior,
    focus_handle: FocusHandle,
    restore_selected_branch: Option<SharedString>,
    show_footer: bool,
    hovered_delete_index: Option<usize>,
}

enum BranchSelectionBehavior {
    Checkout,
    Select {
        selected_branch: Option<SharedString>,
        on_select: SelectBranchCallback,
    },
}

impl BranchSelectionBehavior {
    fn selected_branch(&self) -> Option<&SharedString> {
        match self {
            Self::Checkout => None,
            Self::Select {
                selected_branch, ..
            } => selected_branch.as_ref(),
        }
    }

    fn is_select_only(&self) -> bool {
        matches!(self, Self::Select { .. })
    }
}

#[derive(Clone)]
struct BranchSelectionContext {
    selected_branch: Option<SharedString>,
    active_branch_ref_name: Option<SharedString>,
    active_branch_upstream_ref_name: Option<SharedString>,
    active_branch_remote_name: Option<SharedString>,
}

impl BranchSelectionContext {
    fn new(
        selected_branch: Option<SharedString>,
        repo: Option<&Entity<Repository>>,
        cx: &App,
    ) -> Self {
        let active_branch = repo.and_then(|repo| repo.read(cx).branch.clone());
        let active_branch_ref_name = active_branch.as_ref().map(|branch| branch.ref_name.clone());
        let active_branch_upstream_ref_name = active_branch.as_ref().and_then(|branch| {
            branch
                .upstream
                .as_ref()
                .map(|upstream| upstream.ref_name.clone())
        });
        let active_branch_remote_name = active_branch.as_ref().and_then(|branch| {
            branch
                .upstream
                .as_ref()
                .and_then(|upstream| upstream.remote_name())
                .or_else(|| branch.remote_name())
                .map(SharedString::from)
        });

        Self {
            selected_branch,
            active_branch_ref_name,
            active_branch_upstream_ref_name,
            active_branch_remote_name,
        }
    }

    fn priority(&self, branch: &Branch) -> usize {
        if self
            .selected_branch
            .as_ref()
            .is_some_and(|selected_branch| branch_matches_ref(branch, selected_branch))
        {
            0
        } else if self.is_on_active_branch_remote(branch) {
            1
        } else if self.is_active_branch(branch) || self.is_active_upstream(branch) {
            3
        } else {
            2
        }
    }

    fn is_active_branch(&self, branch: &Branch) -> bool {
        self.active_branch_ref_name
            .as_ref()
            .is_some_and(|ref_name| branch.ref_name.as_ref() == ref_name.as_ref())
    }

    fn is_active_upstream(&self, branch: &Branch) -> bool {
        self.active_branch_upstream_ref_name
            .as_ref()
            .is_some_and(|ref_name| branch.ref_name.as_ref() == ref_name.as_ref())
    }

    fn is_on_active_branch_remote(&self, branch: &Branch) -> bool {
        if self.is_active_branch(branch) || self.is_active_upstream(branch) {
            return false;
        }

        let Some(active_branch_remote_name) = &self.active_branch_remote_name else {
            return false;
        };

        branch_remote_name(branch)
            .is_some_and(|remote_name| remote_name == active_branch_remote_name.as_ref())
    }
}

#[derive(Debug)]
enum PickerState {
    /// When we display list of branches/remotes
    List,
    /// When we set an url to create a new remote
    NewRemote,
    /// When we confirm the new remote url (after NewRemote)
    CreateRemote(SharedString),
    /// When we set a new branch to create
    NewBranch,
}

fn delete_branch_command(is_remote: bool, branch_name: &str, force: bool) -> String {
    format!(
        "branch {} {branch_name}",
        delete_branch_flag(is_remote, force)
    )
}

struct BranchDeleteForceDeletePrompt {
    required_error_substrings: &'static [&'static str],
    message: fn(&str) -> String,
}

impl BranchDeleteForceDeletePrompt {
    fn matches(&self, normalized_error_message: &str) -> bool {
        self.required_error_substrings
            .iter()
            .all(|substring| normalized_error_message.contains(substring))
    }
}

const BRANCH_DELETE_FORCE_DELETE_PROMPTS: &[BranchDeleteForceDeletePrompt] =
    &[BranchDeleteForceDeletePrompt {
        required_error_substrings: &["not fully merged"],
        message: unmerged_branch_force_delete_prompt,
    }];

fn unmerged_branch_force_delete_prompt(branch_name: &str) -> String {
    format!("Branch \"{branch_name}\" is not fully merged. Force delete it?")
}

// Git only reports these cases via localized stderr, so this best-effort check
// may miss some locales and fall back to the raw error toast.
fn force_delete_prompt_for_branch_delete_error(
    error: &anyhow::Error,
    branch_name: &str,
) -> Option<String> {
    let normalized_error_message = error.to_string().to_lowercase();
    BRANCH_DELETE_FORCE_DELETE_PROMPTS
        .iter()
        .find(|prompt| prompt.matches(&normalized_error_message))
        .map(|prompt| (prompt.message)(branch_name))
}

struct DeleteBranchTooltip {
    picker: WeakEntity<Picker<BranchListDelegate>>,
    focus_handle: FocusHandle,
    delete_index: usize,
    _subscription: Subscription,
}

impl DeleteBranchTooltip {
    fn new(
        picker: Entity<Picker<BranchListDelegate>>,
        focus_handle: FocusHandle,
        delete_index: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&picker, |_, _, cx| cx.notify());
        Self {
            picker: picker.downgrade(),
            focus_handle,
            delete_index,
            _subscription: subscription,
        }
    }
}

impl Render for DeleteBranchTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let force_delete = self
            .picker
            .read_with(cx, |picker, _| {
                picker
                    .delegate
                    .is_force_delete_hovering_index(self.delete_index)
            })
            .unwrap_or(false);
        if force_delete {
            Tooltip::for_action_in(
                "Force Delete Branch",
                &branch_picker::ForceDeleteBranch,
                &self.focus_handle,
                cx,
            )
            .into_any_element()
        } else {
            Tooltip::with_meta_in(
                "Delete Branch",
                Some(&branch_picker::DeleteBranch),
                "Hold alt to force delete",
                &self.focus_handle,
                cx,
            )
            .into_any_element()
        }
    }
}

fn branch_matches_ref(branch: &Branch, branch_ref: &SharedString) -> bool {
    branch.ref_name.as_ref() == branch_ref.as_ref() || branch.name() == branch_ref.as_ref()
}

// Git branch names can't contain whitespace, so we replace spaces with dashes,
// but we need to first trim because a branch name can't start or end with a
// dash.
fn normalize_branch_name(query: &str) -> String {
    query.trim().replace(' ', "-")
}

fn branch_remote_name(branch: &Branch) -> Option<&str> {
    branch.remote_name().or_else(|| {
        branch
            .upstream
            .as_ref()
            .and_then(|upstream| upstream.remote_name())
    })
}

fn sort_branch_entries(
    matches: &mut [Entry],
    branch_selection_context: Option<&BranchSelectionContext>,
) {
    matches.sort_by_key(|entry| {
        let Some(branch) = entry.as_branch() else {
            return (4, false);
        };

        let priority = branch_selection_context
            .map(|context| context.priority(branch))
            .unwrap_or(0);
        (priority, branch.is_remote())
    });
}

fn process_branches(
    branches: &Arc<[Branch]>,
    preserved_branch: Option<&SharedString>,
) -> Vec<Branch> {
    let remote_upstreams: HashSet<_> = branches
        .iter()
        .filter_map(|branch| {
            branch
                .upstream
                .as_ref()
                .filter(|upstream| upstream.is_remote())
                .map(|upstream| upstream.ref_name.clone())
        })
        .collect();

    let mut result: Vec<Branch> = branches
        .iter()
        .filter(|branch| {
            !remote_upstreams.contains(&branch.ref_name)
                || preserved_branch
                    .as_ref()
                    .is_some_and(|preserved_branch| branch_matches_ref(branch, preserved_branch))
        })
        .cloned()
        .collect();

    result.sort_by_key(|branch| {
        (
            !branch.is_head,
            branch
                .most_recent_commit
                .as_ref()
                .map(|commit| 0 - commit.commit_timestamp),
        )
    });

    result
}

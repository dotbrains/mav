use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use workspace::WorkspaceDb;

pub struct OnboardingPagesDb(ThreadSafeConnection);

impl Domain for OnboardingPagesDb {
    const NAME: &str = stringify!(OnboardingPagesDb);

    const MIGRATIONS: &[&str] = &[
        sql!(
                    CREATE TABLE onboarding_pages (
                        workspace_id INTEGER,
                        item_id INTEGER UNIQUE,
                        page_number INTEGER,

                        PRIMARY KEY(workspace_id, item_id),
                        FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                        ON DELETE CASCADE
                    ) STRICT;
        ),
        sql!(
                    CREATE TABLE onboarding_pages_2 (
                        workspace_id INTEGER,
                        item_id INTEGER UNIQUE,

                        PRIMARY KEY(workspace_id, item_id),
                        FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                        ON DELETE CASCADE
                    ) STRICT;
                    INSERT INTO onboarding_pages_2 SELECT workspace_id, item_id FROM onboarding_pages;
                    DROP TABLE onboarding_pages;
                    ALTER TABLE onboarding_pages_2 RENAME TO onboarding_pages;
        ),
    ];
}

db::static_connection!(OnboardingPagesDb, [WorkspaceDb]);

impl OnboardingPagesDb {
    query! {
        pub async fn save_onboarding_page(
            item_id: workspace::ItemId,
            workspace_id: workspace::WorkspaceId
        ) -> Result<()> {
            INSERT OR REPLACE INTO onboarding_pages(item_id, workspace_id)
            VALUES (?, ?)
        }
    }

    query! {
        pub fn get_onboarding_page(
            item_id: workspace::ItemId,
            workspace_id: workspace::WorkspaceId
        ) -> Result<Option<workspace::ItemId>> {
            SELECT item_id
            FROM onboarding_pages
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}

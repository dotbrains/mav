use super::*;

impl WorkspaceDb {
    pub(super) fn get_center_pane_group(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<SerializedPaneGroup> {
        Ok(self
            .get_pane_group(workspace_id, None)?
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                SerializedPaneGroup::Pane(SerializedPane {
                    active: true,
                    children: vec![],
                    pinned_count: 0,
                    kind: PaneKind::Tabs,
                    visible: true,
                })
            }))
    }

    fn get_pane_group(
        &self,
        workspace_id: WorkspaceId,
        group_id: Option<GroupId>,
    ) -> Result<Vec<SerializedPaneGroup>> {
        type GroupKey = (Option<GroupId>, WorkspaceId);
        type GroupOrPane = (
            Option<GroupId>,
            Option<SerializedAxis>,
            Option<PaneId>,
            Option<bool>,
            Option<usize>,
            Option<String>,
            Option<PaneKind>,
            Option<bool>,
        );
        self.select_bound::<GroupKey, GroupOrPane>(sql!(
            SELECT group_id, axis, pane_id, active, pinned_count, flexes, kind, visible
                FROM (SELECT
                        group_id,
                        axis,
                        NULL as pane_id,
                        NULL as active,
                        NULL as pinned_count,
                        NULL as kind,
                        NULL as visible,
                        position,
                        parent_group_id,
                        workspace_id,
                        flexes
                      FROM pane_groups
                    UNION
                      SELECT
                        NULL,
                        NULL,
                        center_panes.pane_id,
                        panes.active as active,
                        pinned_count,
                        panes.kind as kind,
                        panes.visible as visible,
                        position,
                        parent_group_id,
                        panes.workspace_id as workspace_id,
                        NULL
                      FROM center_panes
                      JOIN panes ON center_panes.pane_id = panes.pane_id)
                WHERE parent_group_id IS ? AND workspace_id = ?
                ORDER BY position
        ))?((group_id, workspace_id))?
        .into_iter()
        .map(
            |(group_id, axis, pane_id, active, pinned_count, flexes, pane_kind, visible)| {
                let maybe_pane =
                    maybe!({ Some((pane_id?, active?, pinned_count?, pane_kind?, visible?)) });
                if let Some((group_id, axis)) = group_id.zip(axis) {
                    let flexes = flexes
                        .map(|flexes: String| serde_json::from_str::<Vec<f32>>(&flexes))
                        .transpose()?;

                    Ok(SerializedPaneGroup::Group {
                        axis,
                        children: self.get_pane_group(workspace_id, Some(group_id))?,
                        flexes,
                    })
                } else if let Some((pane_id, active, pinned_count, pane_kind, visible)) = maybe_pane
                {
                    Ok(SerializedPaneGroup::Pane(
                        SerializedPane::new_with_kind(
                            self.get_items(pane_id)?,
                            active,
                            pinned_count,
                            pane_kind,
                        )
                        .with_visible(visible),
                    ))
                } else {
                    bail!("Pane Group Child was neither a pane group or a pane");
                }
            },
        )
        // Filter out panes and pane groups which don't have any children or items
        .filter(|pane_group| match pane_group {
            Ok(SerializedPaneGroup::Group { children, .. }) => !children.is_empty(),
            Ok(SerializedPaneGroup::Pane(pane)) => {
                !pane.children.is_empty() || !pane.kind.is_tabbed()
            }
            _ => true,
        })
        .collect::<Result<_>>()
    }

    pub(super) fn save_pane_group(
        conn: &Connection,
        workspace_id: WorkspaceId,
        pane_group: &SerializedPaneGroup,
        parent: Option<(GroupId, usize)>,
    ) -> Result<()> {
        if parent.is_none() {
            log::debug!("Saving a pane group for workspace {workspace_id:?}");
        }
        match pane_group {
            SerializedPaneGroup::Group {
                axis,
                children,
                flexes,
            } => {
                let (parent_id, position) = parent.unzip();

                let flex_string = flexes
                    .as_ref()
                    .map(|flexes| serde_json::json!(flexes).to_string());

                let group_id = conn.select_row_bound::<_, i64>(sql!(
                    INSERT INTO pane_groups(
                        workspace_id,
                        parent_group_id,
                        position,
                        axis,
                        flexes
                    )
                    VALUES (?, ?, ?, ?, ?)
                    RETURNING group_id
                ))?((
                    workspace_id,
                    parent_id,
                    position,
                    *axis,
                    flex_string,
                ))?
                .context("Couldn't retrieve group_id from inserted pane_group")?;

                for (position, group) in children.iter().enumerate() {
                    Self::save_pane_group(conn, workspace_id, group, Some((group_id, position)))?
                }

                Ok(())
            }
            SerializedPaneGroup::Pane(pane) => {
                Self::save_pane(conn, workspace_id, pane, parent)?;
                Ok(())
            }
        }
    }

    fn save_pane(
        conn: &Connection,
        workspace_id: WorkspaceId,
        pane: &SerializedPane,
        parent: Option<(GroupId, usize)>,
    ) -> Result<PaneId> {
        let pane_id = conn.select_row_bound::<_, i64>(sql!(
            INSERT INTO panes(workspace_id, active, pinned_count, kind, visible)
            VALUES (?, ?, ?, ?, ?)
            RETURNING pane_id
        ))?((
            workspace_id,
            pane.active,
            pane.pinned_count,
            pane.kind,
            pane.visible,
        ))?
        .context("Could not retrieve inserted pane_id")?;

        let (parent_id, order) = parent.unzip();
        conn.exec_bound(sql!(
            INSERT INTO center_panes(pane_id, parent_group_id, position)
            VALUES (?, ?, ?)
        ))?((pane_id, parent_id, order))?;

        Self::save_items(conn, workspace_id, pane_id, &pane.children).context("Saving items")?;

        Ok(pane_id)
    }

    fn get_items(&self, pane_id: PaneId) -> Result<Vec<SerializedItem>> {
        self.select_bound(sql!(
            SELECT kind, item_id, active, preview FROM items
            WHERE pane_id = ?
                ORDER BY position
        ))?(pane_id)
    }

    fn save_items(
        conn: &Connection,
        workspace_id: WorkspaceId,
        pane_id: PaneId,
        items: &[SerializedItem],
    ) -> Result<()> {
        let mut insert = conn.exec_bound(sql!(
            INSERT INTO items(workspace_id, pane_id, position, kind, item_id, active, preview) VALUES (?, ?, ?, ?, ?, ?, ?)
        )).context("Preparing insertion")?;
        for (position, item) in items.iter().enumerate() {
            insert((workspace_id, pane_id, position, item))?;
        }

        Ok(())
    }
}

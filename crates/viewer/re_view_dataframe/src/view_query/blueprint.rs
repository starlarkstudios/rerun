use std::collections::HashSet;

use re_chunk_store::ColumnDescriptor;
use re_log_types::{EntityPath, ResolvedTimeRange, Timeline, TimelineName};
use re_sorbet::{ColumnSelector, ComponentColumnSelector};
use re_types::blueprint::{components, datatypes};
use re_viewer_context::{ViewSystemExecutionError, ViewerContext};

use crate::dataframe_ui::HideColumnAction;
use crate::view_query::Query;

// Accessors wrapping reads/writes to the blueprint store.
impl Query {
    /// Get the query timeline name.
    ///
    /// This dis-regards whether a timeline actually exists with this name.
    pub(crate) fn timeline_name(
        &self,
        ctx: &ViewerContext<'_>,
    ) -> Result<re_log_types::TimelineName, ViewSystemExecutionError> {
        let timeline_name = self
            .query_property
            .component_or_empty::<components::TimelineName>()?;

        // if the timeline is unset, we "freeze" it to the current time panel timeline
        if let Some(timeline_name) = timeline_name {
            Ok(timeline_name.into())
        } else {
            let timeline_name = *ctx.rec_cfg.time_ctrl.read().timeline().name();
            self.save_timeline_name(ctx, &timeline_name);

            Ok(timeline_name)
        }
    }

    /// Get the query timeline.
    ///
    /// This returns the query timeline if it actually exists, or `None` otherwise.
    pub fn timeline(
        &self,
        ctx: &ViewerContext<'_>,
    ) -> Result<Option<Timeline>, ViewSystemExecutionError> {
        let timeline_name = self.timeline_name(ctx)?;

        Ok(ctx.recording().timelines().get(&timeline_name).copied())
    }

    /// Save the timeline to the one specified.
    ///
    /// Note: this resets the range filter timestamps to -inf/+inf as any other value might be
    /// invalidated.
    pub fn save_timeline_name(&self, ctx: &ViewerContext<'_>, timeline_name: &TimelineName) {
        self.query_property
            .save_blueprint_component(ctx, &components::TimelineName::from(timeline_name.as_str()));

        // clearing the range filter is equivalent to setting it to the default -inf/+inf
        self.query_property
            .clear_blueprint_component::<components::FilterByRange>(ctx);
    }

    pub fn filter_by_range(&self) -> Result<ResolvedTimeRange, ViewSystemExecutionError> {
        Ok(self
            .query_property
            .component_or_empty::<components::FilterByRange>()?
            .map(|range_filter| (ResolvedTimeRange::new(range_filter.start, range_filter.end)))
            .unwrap_or(ResolvedTimeRange::EVERYTHING))
    }

    pub fn save_filter_by_range(&self, ctx: &ViewerContext<'_>, range: ResolvedTimeRange) {
        if range == ResolvedTimeRange::EVERYTHING {
            self.query_property
                .clear_blueprint_component::<components::FilterByRange>(ctx);
        } else {
            self.query_property.save_blueprint_component(
                ctx,
                &components::FilterByRange::new(range.min(), range.max()),
            );
        }
    }

    /// Get the filter column for the filter-is-not-null feature, if active.
    pub fn filter_is_not_null(
        &self,
    ) -> Result<Option<ComponentColumnSelector>, ViewSystemExecutionError> {
        Ok(self
            .filter_is_not_null_raw()?
            .filter(|filter_is_not_null| filter_is_not_null.active())
            .map(|filter| {
                ComponentColumnSelector::new_for_component_name(
                    filter.entity_path(),
                    filter.component_name(),
                )
            }))
    }

    /// Get the raw [`components::FilterIsNotNull`] struct (for ui purposes).
    pub fn filter_is_not_null_raw(
        &self,
    ) -> Result<Option<components::FilterIsNotNull>, ViewSystemExecutionError> {
        Ok(self
            .query_property
            .component_or_empty::<components::FilterIsNotNull>()?)
    }

    pub fn save_filter_is_not_null(
        &self,
        ctx: &ViewerContext<'_>,
        filter_is_not_null: &components::FilterIsNotNull,
    ) {
        self.query_property
            .save_blueprint_component(ctx, filter_is_not_null);
    }

    pub fn latest_at_enabled(&self) -> Result<bool, ViewSystemExecutionError> {
        Ok(self
            .query_property
            .component_or_empty::<components::ApplyLatestAt>()?
            .is_some_and(|comp| *comp.0))
    }

    pub fn save_latest_at_enabled(&self, ctx: &ViewerContext<'_>, enabled: bool) {
        self.query_property
            .save_blueprint_component(ctx, &components::ApplyLatestAt(enabled.into()));
    }

    pub fn save_selected_columns(
        &self,
        ctx: &ViewerContext<'_>,
        columns: impl IntoIterator<Item = ColumnSelector>,
    ) {
        let mut selected_columns = datatypes::SelectedColumns::default();
        for column in columns {
            match column {
                ColumnSelector::Time(desc) => {
                    selected_columns
                        .time_columns
                        .push(desc.timeline.as_str().into());
                }

                ColumnSelector::Component(selector) => {
                    let blueprint_component_selector = datatypes::ComponentColumnSelector::new(
                        &selector.entity_path,
                        &selector.component_name,
                    );

                    selected_columns
                        .component_columns
                        .push(blueprint_component_selector);
                }
            }
        }

        self.query_property
            .save_blueprint_component(ctx, &components::SelectedColumns(selected_columns));
    }

    pub fn save_all_columns_selected(&self, ctx: &ViewerContext<'_>) {
        self.query_property
            .clear_blueprint_component::<components::SelectedColumns>(ctx);
    }

    pub fn save_all_columns_unselected(&self, ctx: &ViewerContext<'_>) {
        self.query_property
            .save_blueprint_component(ctx, &components::SelectedColumns::default());
    }

    /// Given some view columns, list the columns that should be visible (aka "selected columns"),
    /// according to the blueprint.
    ///
    /// This operates by filtering the view columns based on the blueprint specified columns.
    ///
    /// Returns `Ok(None)` if all columns should be displayed (aka a column selection isn't provided
    /// in the blueprint).
    pub fn apply_column_visibility_to_view_columns(
        &self,
        ctx: &ViewerContext<'_>,
        view_columns: &[ColumnDescriptor],
    ) -> Result<Option<Vec<ColumnSelector>>, ViewSystemExecutionError> {
        let selected_columns = self
            .query_property
            .component_or_empty::<components::SelectedColumns>()?;

        // no selected columns means all columns are visible
        let Some(datatypes::SelectedColumns {
            time_columns,
            component_columns,
        }) = selected_columns.as_deref()
        else {
            // select all columns
            return Ok(None);
        };

        let selected_time_columns: HashSet<TimelineName> = time_columns
            .iter()
            .map(|timeline_name| timeline_name.as_str().into())
            .collect();
        let selected_component_columns = component_columns
            .iter()
            .map(|selector| {
                (
                    EntityPath::from(selector.entity_path.as_str()),
                    selector.component.as_str(),
                )
            })
            .collect::<HashSet<_>>();

        let query_timeline_name = self.timeline_name(ctx)?;
        let result = view_columns
            .iter()
            .filter(|column| match column {
                ColumnDescriptor::Time(desc) => {
                    // we always include the query timeline column because we need it for the dataframe ui
                    desc.timeline_name() == query_timeline_name
                        || selected_time_columns.contains(&desc.timeline_name())
                }

                ColumnDescriptor::Component(desc) => {
                    // Check against both the full name and short name, as the user might have used
                    // the latter in the blueprint API.
                    //
                    // TODO(ab): this means that if the user chooses `"/foo/bar:Scalar"`, it will
                    // select both `rerun.components.Scalar` and `Scalar`, should both of these
                    // exist.
                    selected_component_columns
                        .contains(&(desc.entity_path.clone(), desc.component_name.full_name()))
                        || selected_component_columns
                            .contains(&(desc.entity_path.clone(), desc.component_name.short_name()))
                }
            })
            .cloned()
            .map(ColumnSelector::from)
            .collect();

        Ok(Some(result))
    }

    pub(crate) fn handle_hide_column_actions(
        &self,
        ctx: &ViewerContext<'_>,
        view_columns: &[ColumnDescriptor],
        actions: Vec<HideColumnAction>,
    ) -> Result<(), ViewSystemExecutionError> {
        if actions.is_empty() {
            return Ok(());
        }

        let mut selected_columns: Vec<_> = self
            .apply_column_visibility_to_view_columns(ctx, view_columns)?
            .map(|columns| columns.into_iter().collect())
            .unwrap_or_else(|| view_columns.iter().cloned().map(Into::into).collect());

        for action in actions {
            match action {
                HideColumnAction::HideTimeColumn { timeline_name } => {
                    selected_columns.retain(|column| match column {
                        ColumnSelector::Time(desc) => desc.timeline != timeline_name,
                        ColumnSelector::Component(_) => true,
                    });
                }

                HideColumnAction::HideComponentColumn {
                    entity_path,
                    component_name,
                } => {
                    selected_columns.retain(|column| match column {
                        ColumnSelector::Component(selector) => {
                            selector.entity_path != entity_path
                                || !component_name.matches(&selector.component_name)
                        }
                        ColumnSelector::Time(_) => true,
                    });
                }
            }
        }

        self.save_selected_columns(ctx, selected_columns);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::Query;
    use re_viewer_context::test_context::TestContext;
    use re_viewer_context::ViewId;

    /// Simple test to demo round-trip testing using [`TestContext::run_and_handle_system_commands`].
    #[test]
    fn test_latest_at_enabled() {
        let mut test_context = TestContext::default();

        let view_id = ViewId::random();

        test_context.run_in_egui_central_panel(|ctx, _| {
            let query = Query::from_blueprint(ctx, view_id);
            query.save_latest_at_enabled(ctx, true);
        });
        test_context.handle_system_commands();

        test_context.run_in_egui_central_panel(|ctx, _| {
            let query = Query::from_blueprint(ctx, view_id);
            assert!(query.latest_at_enabled().unwrap());
        });
    }
}

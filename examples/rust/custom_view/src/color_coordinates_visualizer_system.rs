use rerun::external::{
    egui,
    re_log_types::{EntityPath, Instance},
    re_renderer,
    re_viewer_context::{
        self, IdentifiedViewSystem, ViewContext, ViewContextCollection, ViewQuery,
        ViewSystemExecutionError, ViewSystemIdentifier, VisualizerQueryInfo, VisualizerSystem,
    },
};
use rerun::Component as _;

use crate::color_archetype::ColorArchetype;

/// Our view consist of single part which holds a list of egui colors for each entity path.
#[derive(Default)]
pub struct InstanceColorSystem {
    pub colors: Vec<(EntityPath, Vec<ColorWithInstance>)>,
}

pub struct ColorWithInstance {
    pub color: egui::Color32,
    pub instance: Instance,
}

impl IdentifiedViewSystem for InstanceColorSystem {
    fn identifier() -> ViewSystemIdentifier {
        "InstanceColor".into()
    }
}

impl VisualizerSystem for InstanceColorSystem {
    fn visualizer_query_info(&self) -> VisualizerQueryInfo {
        VisualizerQueryInfo::from_archetype::<ColorArchetype>()
    }

    /// Populates the visualizer with data from the store.
    fn execute(
        &mut self,
        ctx: &ViewContext<'_>,
        query: &ViewQuery<'_>,
        _context_systems: &ViewContextCollection,
    ) -> Result<Vec<re_renderer::QueueableDrawData>, ViewSystemExecutionError> {
        // For each entity in the view that should be displayed with the `InstanceColorSystem`…
        for data_result in query.iter_visible_data_results(Self::identifier()) {
            // …gather all colors and their instance ids.

            let results = ctx.recording_engine().cache().latest_at(
                &ctx.current_query(),
                &data_result.entity_path,
                [rerun::Color::name()],
            );

            let Some(colors) = results.component_batch::<rerun::Color>() else {
                continue;
            };

            self.colors.push((
                data_result.entity_path.clone(),
                (0..)
                    .zip(colors)
                    .map(|(instance, color)| {
                        let [r, g, b, _] = color.to_array();
                        ColorWithInstance {
                            color: egui::Color32::from_rgb(r, g, b),
                            instance: instance.into(),
                        }
                    })
                    .collect(),
            ));
        }

        // We're not using `re_renderer` here, so return an empty vector.
        // If you want to draw additional primitives here, you can emit re_renderer draw data here directly.
        Ok(Vec::new())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn fallback_provider(&self) -> &dyn re_viewer_context::ComponentFallbackProvider {
        self
    }
}

// Implements a `ComponentFallbackProvider` trait for the `InstanceColorSystem`.
// It is left empty here but could be used to provides fallback values for optional components in case they're missing.
use rerun::external::re_types;
re_viewer_context::impl_component_fallback_provider!(InstanceColorSystem => []);
